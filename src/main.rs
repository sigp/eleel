use crate::{
    config::Config,
    jwt::{jwt_secret_from_path, verify_single_token, KeyCollection, Secret},
    multiplexer::Multiplexer,
    types::{
        ErrorResponse, MaybeErrorResponse, Request, Requests, Response, Responses, TaskExecutor,
    },
};
use axum::{
    extract::{rejection::JsonRejection, DefaultBodyLimit, State},
    headers::{authorization::Bearer, Authorization},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router, TypedHeader,
};
use clap::Parser;
use eth2::types::MainnetEthSpec;
use execution_layer::http::{
    ENGINE_EXCHANGE_CAPABILITIES, ENGINE_FORKCHOICE_UPDATED_V1, ENGINE_FORKCHOICE_UPDATED_V2,
    ENGINE_FORKCHOICE_UPDATED_V3, ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1,
    ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1, ENGINE_GET_PAYLOAD_V1, ENGINE_GET_PAYLOAD_V2,
    ENGINE_GET_PAYLOAD_V3, ENGINE_NEW_PAYLOAD_V1, ENGINE_NEW_PAYLOAD_V2, ENGINE_NEW_PAYLOAD_V3,
    ETH_SYNCING,
};
use slog::Logger;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::runtime::Handle;

mod base_fee;
mod config;
mod fcu;
mod jwt;
mod logging;
mod meta;
mod multiplexer;
mod new_payload;
mod payload_builder;
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
    let controller_jwt_secret = jwt_secret_from_path(&config.controller_jwt_secret).unwrap();
    let client_jwt_collection = KeyCollection::load(&config.client_jwt_secrets).unwrap();
    let multiplexer = Multiplexer::<E>::new(config, executor, log).await.unwrap();
    let app_state = Arc::new(AppState {
        controller_jwt_secret,
        client_jwt_collection,
        multiplexer,
    });

    let app = Router::new()
        .route("/", post(handle_client_json_rpc))
        .route("/canonical", post(handle_controller_json_rpc))
        .route("/health", get(handle_health))
        .with_state(app_state)
        .layer(DefaultBodyLimit::max(body_limit_mb * MEGABYTE));

    let addr = SocketAddr::from((listen_address, listen_port));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

struct AppState {
    controller_jwt_secret: Secret,
    client_jwt_collection: KeyCollection,
    multiplexer: Multiplexer<E>,
}

// TODO: do something with signal/signal_rx
async fn new_task_executor(log: Logger) -> TaskExecutor {
    let handle = Handle::current();
    let (_signal, exit) = async_channel::bounded(1);
    let (shutdown_tx, _) = futures::channel::mpsc::channel(1);
    TaskExecutor::new(handle, exit, log, shutdown_tx)
}

async fn handle_client_json_rpc(
    State(state): State<Arc<AppState>>,
    TypedHeader(jwt_token_str): TypedHeader<Authorization<Bearer>>,
    maybe_requests: Result<Json<Requests>, JsonRejection>,
) -> Json<Responses> {
    let jwt_key_collection = &state.client_jwt_collection;
    let multiplexer = &state.multiplexer;

    // Check JWT auth.
    if let Err(e) = jwt_key_collection.verify(jwt_token_str.token()) {
        tracing::warn!(
            error = ?e,
            "JWT auth failed"
        );
        return Json(Responses::Single(MaybeErrorResponse::Err(
            ErrorResponse::parse_error_generic(serde_json::json!(0), e),
        )));
    }

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
            process_client_request(multiplexer, request).await.into(),
        )),
        Requests::Multiple(requests) => {
            let mut results = vec![];

            for request in requests {
                results.push(process_client_request(multiplexer, request).await.into());
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
        ENGINE_FORKCHOICE_UPDATED_V1
        | ENGINE_FORKCHOICE_UPDATED_V2
        | ENGINE_FORKCHOICE_UPDATED_V3 => multiplexer.handle_fcu(request).await,
        ENGINE_NEW_PAYLOAD_V1 | ENGINE_NEW_PAYLOAD_V2 | ENGINE_NEW_PAYLOAD_V3 => {
            multiplexer.handle_new_payload(request).await
        }
        ETH_SYNCING => multiplexer.handle_syncing(request).await,
        "eth_chainId" => multiplexer.handle_chain_id(request).await,
        ENGINE_EXCHANGE_CAPABILITIES => multiplexer.handle_engine_capabilities(request).await,
        "eth_getBlockByNumber"
        | "eth_getBlockByHash"
        | "eth_getLogs"
        | "eth_call"
        | "eth_blockNumber"
        | ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1
        | ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1 => multiplexer.proxy_directly(request).await,
        ENGINE_GET_PAYLOAD_V1 | ENGINE_GET_PAYLOAD_V2 | ENGINE_GET_PAYLOAD_V3 => {
            multiplexer.handle_get_payload(request).await
        }
        method => Err(ErrorResponse::unsupported_method(request.id, method)),
    }
}

async fn handle_controller_json_rpc(
    State(state): State<Arc<AppState>>,
    TypedHeader(jwt_token_str): TypedHeader<Authorization<Bearer>>,
    maybe_request: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Response>, Json<ErrorResponse>> {
    let jwt_secret = &state.controller_jwt_secret;
    let multiplexer = &state.multiplexer;

    // Check JWT auth.
    if let Err(e) = verify_single_token(jwt_token_str.token(), jwt_secret) {
        tracing::warn!(
            error = ?e,
            "Controller JWT auth failed"
        );
        return Err(Json(ErrorResponse::parse_error_generic(
            serde_json::json!(0),
            e,
        )));
    }

    let Json(request) = maybe_request
        .map_err(|e| ErrorResponse::parse_error_generic(serde_json::json!(0), e.body_text()))?;

    match request.method.as_str() {
        ENGINE_FORKCHOICE_UPDATED_V1
        | ENGINE_FORKCHOICE_UPDATED_V2
        | ENGINE_FORKCHOICE_UPDATED_V3 => multiplexer.handle_controller_fcu(request).await,
        ENGINE_NEW_PAYLOAD_V1 | ENGINE_NEW_PAYLOAD_V2 | ENGINE_NEW_PAYLOAD_V3 => {
            multiplexer.handle_controller_new_payload(request).await
        }
        ETH_SYNCING => multiplexer.handle_syncing(request).await,
        "eth_chainId" => multiplexer.handle_chain_id(request).await,
        ENGINE_EXCHANGE_CAPABILITIES => multiplexer.handle_engine_capabilities(request).await,
        "eth_getBlockByNumber"
        | "eth_getBlockByHash"
        | "eth_getLogs"
        | "eth_call"
        | "eth_blockNumber"
        | ENGINE_GET_PAYLOAD_BODIES_BY_HASH_V1
        | ENGINE_GET_PAYLOAD_BODIES_BY_RANGE_V1 => multiplexer.proxy_directly(request).await,
        ENGINE_GET_PAYLOAD_V1 | ENGINE_GET_PAYLOAD_V2 | ENGINE_GET_PAYLOAD_V3 => {
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
