use crate::{
    multiplexer::Multiplexer,
    types::{ErrorResponse, QuantityU64, Request, Response},
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
}
