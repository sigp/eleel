//! Handler for forkchoiceUpdated.
use crate::{
    multiplexer::Multiplexer,
    types::{
        ErrorResponse, JsonForkchoiceStateV1, JsonForkchoiceUpdatedV1Response, JsonPayloadStatusV1,
        JsonPayloadStatusV1Status, JsonValue, Request, Response,
    },
};
use eth2::types::EthSpec;
use std::time::{Duration, Instant};

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_controller_fcu(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, (fcu, _payload_attributes)) =
            request.parse_as::<(JsonForkchoiceStateV1, JsonValue)>()?;

        let head_hash = fcu.head_block_hash;
        tracing::info!(head_hash = ?head_hash, "processing fcU from controller");

        let response = if let Some(response) = self.get_cached_fcu(&fcu, true).await {
            response
        } else {
            // Make a corresponding request to the EL.
            // Never send payload attributes.
            match self
                .engine
                .notify_forkchoice_updated(fcu.clone().into(), None, &self.log)
                .await
            {
                Ok(response) => {
                    let json_response = JsonForkchoiceUpdatedV1Response::from(response);
                    let status = json_response.payload_status.status;

                    let mut cache = self.fcu_cache.lock().await;

                    let cached = if let Some(existing_entry) = cache.get_mut(&fcu) {
                        if Self::is_definite(&existing_entry.payload_status) {
                            tracing::debug!(
                                head_hash = ?head_hash,
                                "ignoring redundant fcU cache update"
                            );
                            false
                        } else {
                            *existing_entry = json_response.clone();
                            true
                        }
                    } else {
                        cache.put(fcu.clone(), json_response.clone());
                        true
                    };
                    drop(cache);

                    if cached {
                        tracing::info!(
                            head_hash = ?head_hash,
                            status = ?status,
                            "cached fcU from controller"
                        );
                    }

                    json_response
                }
                Err(e) => {
                    // Return an error to the controlling CL.
                    tracing::warn!(error = ?e, "error during fcU");
                    return Err(ErrorResponse::invalid_request(
                        id,
                        format!("forkchoice update failed: see eleel logs"),
                    ));
                }
            }
        };

        Response::new(id, response)
    }

    pub async fn handle_fcu(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, (fcu, _payload_attributes)) =
            request.parse_as::<(JsonForkchoiceStateV1, JsonValue)>()?;

        let head_hash = fcu.head_block_hash;
        tracing::info!(id = ?id, head_hash = ?head_hash, "processing fcU from client");

        // Wait a short time for a definite response from the EL. Chances are it's busy processing
        // the fcU sent by the controlling BN.
        let start = Instant::now();
        while start.elapsed().as_millis() < self.config.fcu_wait_millis {
            if let Some(response) = self.get_cached_fcu(&fcu, true).await {
                return Response::new(id, response);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Check cache, allowing for indefinite Syncing/Accepted responses.
        let response = if let Some(response) = self.get_cached_fcu(&fcu, false).await {
            if !Self::is_definite(&response.payload_status) {
                tracing::info!("sending cached indefinite status on fcU");
            }
            response
        } else {
            // Synthesise a syncing response to send, but do not cache it.
            tracing::info!(id = ?id, head_hash = ?head_hash, "sending SYNCING status on fcU");
            JsonForkchoiceUpdatedV1Response {
                payload_status: JsonPayloadStatusV1 {
                    status: JsonPayloadStatusV1Status::Syncing,
                    latest_valid_hash: None,
                    validation_error: None,
                },
                payload_id: None,
            }
        };
        Response::new(id, response)
    }

    /// Get fcU from cache.
    ///
    /// Definite (valid/invalid) responses may be requested by setting `definite_only=true`.
    pub async fn get_cached_fcu(
        &self,
        fcu: &JsonForkchoiceStateV1,
        definite_only: bool,
    ) -> Option<JsonForkchoiceUpdatedV1Response> {
        let mut cache = self.fcu_cache.lock().await;
        if let Some(existing_response) = cache.get(fcu) {
            if !definite_only || Self::is_definite(&existing_response.payload_status) {
                return Some(existing_response.clone());
            }
        }
        None
    }

    pub fn is_definite(status: &JsonPayloadStatusV1) -> bool {
        use JsonPayloadStatusV1Status::*;
        match status.status {
            Valid | Invalid | InvalidBlockHash => true,
            Accepted | Syncing => false,
        }
    }
}
