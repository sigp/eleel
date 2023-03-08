//! Support for meta methods which return information about the EL itself.
use crate::{
    multiplexer::Multiplexer,
    types::{ErrorResponse, JsonValue, QuantityU64, Request, Response},
};
use eth2::types::EthSpec;
use std::time::Duration;

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_syncing(&self, request: Request) -> Result<Response, ErrorResponse> {
        // TODO: actually check EL status, maybe with a cache
        let (id, _) = request.parse_as::<Vec<()>>()?;
        Response::new(id, false)
    }

    pub async fn handle_chain_id(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, _) = request.parse_as::<Vec<()>>()?;

        // TODO: dynamic timeout
        let timeout = Duration::from_secs(1);
        let chain_id = self
            .engine
            .api
            .get_chain_id(timeout)
            .await
            .map_err(|e| ErrorResponse::parse_error_generic(id.clone(), format!("{e:?}")))?;
        let result = QuantityU64 {
            value: chain_id.into(),
        };
        Response::new(id, result)
    }

    pub async fn handle_engine_capabilities(
        &self,
        request: Request,
    ) -> Result<Response, ErrorResponse> {
        let (id, (_cl_capabilities,)) = request.parse_as::<(Vec<String>,)>()?;

        let max_age = Duration::from_secs(15 * 60);
        let engine_capabilities = self
            .engine
            .get_engine_capabilities(Some(max_age))
            .await
            .map_err(|e| ErrorResponse::parse_error_generic(id.clone(), format!("{e:?}")))?;
        Response::new(id, engine_capabilities.to_response())
    }

    pub async fn proxy_directly(&self, request: Request) -> Result<Response, ErrorResponse> {
        let id = request.id;

        // TODO: adjust timeout
        let timeout = Duration::from_secs(12);

        let result: JsonValue = self
            .engine
            .api
            .rpc_request(&request.method, request.params, timeout)
            .await
            .map_err(|e| ErrorResponse::parse_error_generic(id.clone(), format!("{e:?}")))?;

        Response::new(id, result)
    }
}
