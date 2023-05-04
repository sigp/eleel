use crate::{
    config::Config,
    multiplexer::Multiplexer,
    transition_config::handle_transition_config,
    types::{
        ErrorResponse, MaybeErrorResponse, Request, Requests, Response, Responses, TaskExecutor,
    },
};
use axum::{
    extract::{rejection::JsonRejection, DefaultBodyLimit, State},
    response::IntoResponse,
    routing::{get, post},
    Json, Router, http::StatusCode,
};
use clap::Parser;
use eth2::types::MainnetEthSpec;
use execution_layer::http::{
    ENGINE_EXCHANGE_CAPABILITIES, ENGINE_EXCHANGE_TRANSITION_CONFIGURATION_V1,
    ENGINE_FORKCHOICE_UPDATED_V1, ENGINE_FORKCHOICE_UPDATED_V2,
    ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1, ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1,
    ENGINE_GET_PAYLOAD_V1, ENGINE_GET_PAYLOAD_V2, ENGINE_NEW_PAYLOAD_V1, ENGINE_NEW_PAYLOAD_V2,
    ETH_SYNCING,
};
use futures::channel::mpsc::channel;
use slog::Logger;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::runtime::Handle;

mod config;
mod fcu;
mod logging;
mod meta;
mod multiplexer;
mod new_payload;
mod payload_builder;
mod transition_config;
mod types;

// TODO: allow other specs
type E = MainnetEthSpec;

const MEGABYTE: usize = 1024 * 1024;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let log = crate::logging::new_logger();
    let executor = new_task_executor(log.clone()).await;

    let config = Config::parse();

    let body_limit_mb = config.body_limit_mb;
    let listen_address = config.listen_address;
    let listen_port = config.listen_port;
    let multiplexer = Arc::new(Multiplexer::<E>::new(config, executor, log).unwrap());

    let app = Router::new()
        .route("/", post(handle_client_json_rpc))
        .route("/canonical", post(handle_controller_json_rpc))
        .route("/health", get(handle_health))
        .with_state(multiplexer)
        .layer(DefaultBodyLimit::max(body_limit_mb * MEGABYTE));

    let addr = SocketAddr::from((listen_address, listen_port));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// TODO: do something with signal/signal_rx
async fn new_task_executor(log: Logger) -> TaskExecutor {
    let handle = Handle::current();
    let (_signal, exit) = exit_future::signal();
    let (signal_tx, _signal_rx) = channel(1);
    TaskExecutor::new(handle, exit, log, signal_tx)
}

async fn handle_client_json_rpc(
    State(multiplexer): State<Arc<Multiplexer<E>>>,
    maybe_requests: Result<Json<Requests>, JsonRejection>,
) -> Json<Responses> {
    let requests = match maybe_requests {
        Ok(Json(requests)) => requests,
        Err(e) => {
            return Json(Responses::Single(MaybeErrorResponse::Err(
                ErrorResponse::parse_error_generic(serde_json::json!(0), e.body_text()),
            )));
        }
    };

    match requests {
        Requests::Single(request) => Json(Responses::Single(
            process_client_request(&multiplexer, request).await.into(),
        )),
        Requests::Multiple(requests) => {
            let mut results = vec![];

            for request in requests {
                results.push(process_client_request(&multiplexer, request).await.into());
            }

            Json(Responses::Multiple(results))
        }
    }
}

async fn process_client_request(
    multiplexer: &Multiplexer<E>,
    request: Request,
) -> Result<Response, ErrorResponse> {
    match request.method.as_str() {
        ENGINE_FORKCHOICE_UPDATED_V1 | ENGINE_FORKCHOICE_UPDATED_V2 => {
            multiplexer.handle_fcu(request).await
        }
        ENGINE_NEW_PAYLOAD_V1 | ENGINE_NEW_PAYLOAD_V2 => {
            multiplexer.handle_new_payload(request).await
        }
        ENGINE_EXCHANGE_TRANSITION_CONFIGURATION_V1 => handle_transition_config(request).await,
        ETH_SYNCING => multiplexer.handle_syncing(request).await,
        "eth_chainId" => multiplexer.handle_chain_id(request).await,
        ENGINE_EXCHANGE_CAPABILITIES => multiplexer.handle_engine_capabilities(request).await,
        "eth_getBlockByNumber"
        | "eth_getBlockByHash"
        | "eth_getLogs"
        | "eth_call"
        | ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1
        | ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1 => multiplexer.proxy_directly(request).await,
        ENGINE_GET_PAYLOAD_V1 | ENGINE_GET_PAYLOAD_V2 => {
            multiplexer.handle_get_payload(request).await
        }
        method => Err(ErrorResponse::unsupported_method(request.id, method)),
    }
}

async fn handle_controller_json_rpc(
    State(multiplexer): State<Arc<Multiplexer<E>>>,
    maybe_request: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Response>, Json<ErrorResponse>> {
    let Json(request) = maybe_request
        .map_err(|e| ErrorResponse::parse_error_generic(serde_json::json!(0), e.body_text()))?;

    match request.method.as_str() {
        ENGINE_FORKCHOICE_UPDATED_V1 | ENGINE_FORKCHOICE_UPDATED_V2 => {
            multiplexer.handle_controller_fcu(request).await
        }
        ENGINE_NEW_PAYLOAD_V1 | ENGINE_NEW_PAYLOAD_V2 => {
            multiplexer.handle_controller_new_payload(request).await
        }
        ENGINE_EXCHANGE_TRANSITION_CONFIGURATION_V1 => handle_transition_config(request).await,
        ETH_SYNCING => multiplexer.handle_syncing(request).await,
        "eth_chainId" => multiplexer.handle_chain_id(request).await,
        ENGINE_EXCHANGE_CAPABILITIES => multiplexer.handle_engine_capabilities(request).await,
        "eth_getBlockByNumber"
        | "eth_getBlockByHash"
        | "eth_getLogs"
        | "eth_call"
        | ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1
        | ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1 => multiplexer.proxy_directly(request).await,
        ENGINE_GET_PAYLOAD_V1 | ENGINE_GET_PAYLOAD_V2 => {
            multiplexer.handle_get_payload(request).await
        }
        method => Err(ErrorResponse::unsupported_method(request.id, method)),
    }
    .map(Json)
    .map_err(Json)
}

async fn handle_health() -> impl IntoResponse {
    StatusCode::OK
}
