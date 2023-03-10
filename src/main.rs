use crate::{
    config::Config,
    multiplexer::Multiplexer,
    transition_config::handle_transition_config,
    types::{ErrorResponse, Request, Response, TaskExecutor},
};
use axum::{
    extract::{rejection::JsonRejection, State},
    routing::post,
    Json, Router,
};
use eth2::types::MainnetEthSpec;
use eth2_network_config::Eth2NetworkConfig;
use execution_layer::http::{
    ENGINE_EXCHANGE_CAPABILITIES, ENGINE_EXCHANGE_TRANSITION_CONFIGURATION_V1,
    ENGINE_FORKCHOICE_UPDATED_V1, ENGINE_FORKCHOICE_UPDATED_V2, ENGINE_GET_PAYLOAD_V1,
    ENGINE_GET_PAYLOAD_V2, ENGINE_NEW_PAYLOAD_V1, ENGINE_NEW_PAYLOAD_V2, ETH_SYNCING,
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
mod transition_config;
mod types;

// TODO: allow other specs
type E = MainnetEthSpec;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let log = crate::logging::new_logger();
    let executor = new_task_executor(log.clone()).await;

    // TODO: configurable
    let network_config = Eth2NetworkConfig::constant("mainnet").unwrap().unwrap();

    // TODO: CLI params
    let config = Config {
        el_url: "http://localhost:8551".into(),
        jwt_secret_path: "/tmp/jwtsecret".into(),
        fcu_cache_size: 64,
        new_payload_cache_size: 64,
        network_config,
    };

    let multiplexer = Arc::new(Multiplexer::<E>::new(config, executor, log).unwrap());

    let app = Router::new()
        .route("/", post(handle_client_json_rpc))
        .route("/canonical", post(handle_controller_json_rpc))
        .with_state(multiplexer);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8552));
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
    maybe_request: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Response>, Json<ErrorResponse>> {
    let Json(request) = maybe_request
        .map_err(|e| ErrorResponse::parse_error_generic(serde_json::json!(0), e.body_text()))?;

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
        "eth_getBlockByNumber" | "eth_getBlockByHash" | "eth_getLogs" | "eth_call" => {
            multiplexer.proxy_directly(request).await
        }
        method @ ENGINE_GET_PAYLOAD_V1 | method @ ENGINE_GET_PAYLOAD_V2 => {
            Err(ErrorResponse::unsupported_method(request.id, method))
        }
        method => Err(ErrorResponse::unsupported_method(request.id, method)),
    }
    .map(|response| Json(response))
    .map_err(|err| Json(err))
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
        "eth_getBlockByNumber" | "eth_getBlockByHash" | "eth_getLogs" | "eth_call" => {
            multiplexer.proxy_directly(request).await
        }
        method @ ENGINE_GET_PAYLOAD_V1 | method @ ENGINE_GET_PAYLOAD_V2 => {
            Err(ErrorResponse::unsupported_method(request.id, method))
        }
        method => Err(ErrorResponse::unsupported_method(request.id, method)),
    }
    .map(|response| Json(response))
    .map_err(|err| Json(err))
}
