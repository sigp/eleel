//! Handler for forkchoiceUpdated.
use crate::{
    config::FcuMatching,
    multiplexer::Multiplexer,
    types::{
        ErrorResponse, JsonForkchoiceStateV1, JsonForkchoiceUpdatedV1Response,
        JsonPayloadAttributes, JsonPayloadAttributesV2, JsonPayloadStatusV1,
        JsonPayloadStatusV1Status, JsonValue, Request, Response, TransparentJsonPayloadId,
    },
};
use eth2::types::EthSpec;
use execution_layer::http::ENGINE_FORKCHOICE_UPDATED_V2;
use std::time::{Duration, Instant};

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_controller_fcu(&self, request: Request) -> Result<Response, ErrorResponse> {
        // FIXME: might need ForkVersionDeserialize for payload attributes
        let method_name = request.method.clone();
        let (id, (fcu, json_payload_attributes)) =
            request.parse_as::<(JsonForkchoiceStateV1, Option<JsonValue>)>()?;

        let head_hash = fcu.head_block_hash;
        tracing::info!(head_hash = ?head_hash, "processing fcU from controller");

        let opt_payload_attributes = if method_name == ENGINE_FORKCHOICE_UPDATED_V2 {
            json_payload_attributes
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| {
                    ErrorResponse::parse_error_generic(
                        id.clone(),
                        format!("invalid payload attributes: {e}"),
                    )
                })?
                .map(JsonPayloadAttributes::V2)
        } else {
            json_payload_attributes
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| {
                    ErrorResponse::parse_error_generic(
                        id.clone(),
                        format!("invalid payload attributes: {e}"),
                    )
                })?
                .map(JsonPayloadAttributes::V3)
        };

        let payload_status = if let Some(status) = self.get_cached_fcu(&fcu, true).await {
            status
        } else {
            // Make a corresponding request to the EL.
            // Do not send payload attributes to the EL (for now).
            match self
                .engine
                .notify_forkchoice_updated(fcu.clone().into(), None, &self.log)
                .await
            {
                Ok(response) => {
                    let json_response = JsonForkchoiceUpdatedV1Response::from(response);
                    let status = json_response.payload_status.status;

                    let mut cache = self.fcu_cache.lock().await;

                    let cached = if let Some(existing_status) = cache.get_mut(&fcu) {
                        if Self::is_definite(existing_status) {
                            tracing::debug!(
                                head_hash = ?head_hash,
                                "ignoring redundant fcU cache update"
                            );
                            false
                        } else {
                            *existing_status = json_response.payload_status.clone();
                            true
                        }
                    } else {
                        cache.put(fcu.clone(), json_response.payload_status.clone());
                        true
                    };
                    drop(cache);

                    if cached {
                        tracing::info!(
                            head_hash = ?head_hash,
                            status = ?status,
                            "cached fcU from controller"
                        );

                        if status == JsonPayloadStatusV1Status::Valid {
                            self.justified_block_cache
                                .lock()
                                .await
                                .put(fcu.safe_block_hash, ());
                            self.finalized_block_cache
                                .lock()
                                .await
                                .put(fcu.finalized_block_hash, ());
                        }
                    }

                    json_response.payload_status
                }
                Err(e) => {
                    // Return an error to the controlling CL.
                    tracing::warn!(error = ?e, "error during fcU");
                    return Err(ErrorResponse::invalid_request(
                        id,
                        "forkchoice update failed: see eleel logs".into(),
                    ));
                }
            }
        };

        // FIXME: don't build payload if status is SYNCING/INVALID

        // If the controller sent payload attributes, then register them with the dummy payload
        // builder *even if* the fcU status itself was already cached. This covers the case where
        // the controller initiallysends the fcU without payload attributes, then sends it again
        // later *with* payload attributes.
        let payload_id = if let Some(payload_attributes) = opt_payload_attributes {
            tracing::info!(
                head_hash = ?head_hash,
                "processing payload attributes from controller"
            );
            match self
                .register_attributes(head_hash, payload_attributes.into())
                .await
            {
                Ok(id) => Some(TransparentJsonPayloadId(id)),
                Err(message) => return Err(ErrorResponse::invalid_payload_attributes(id, message)),
            }
        } else {
            None
        };

        let response = JsonForkchoiceUpdatedV1Response {
            payload_status,
            payload_id,
        };

        Response::new(id, response)
    }

    pub async fn handle_fcu(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, (fcu, opt_payload_attributes)) =
            request.parse_as::<(JsonForkchoiceStateV1, Option<JsonPayloadAttributesV2>)>()?;

        let head_hash = fcu.head_block_hash;
        tracing::info!(id = ?id, head_hash = ?head_hash, "processing fcU from client");

        // Wait a short time for a definite response from the EL. Chances are it's busy processing
        // the fcU sent by the controlling BN.
        let mut definite_payload_status = None;
        let start = Instant::now();
        while start.elapsed().as_millis() < self.config.fcu_wait_millis {
            if let Some(definite_status) = self.get_cached_fcu(&fcu, true).await {
                tracing::debug!(id = ?id, head_hash = ?head_hash, "found definite fcU in cache");
                definite_payload_status = Some(definite_status);
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Check cache, allowing for indefinite Syncing/Accepted responses.
        let payload_status = if let Some(definite_status) = definite_payload_status {
            definite_status
        } else if let Some(payload_status) = self.get_cached_fcu(&fcu, false).await {
            if Self::is_definite(&payload_status) {
                tracing::debug!(id = ?id, head_hash = ?head_hash, "found definite fcU in cache");
            } else {
                tracing::info!("sending cached indefinite status on fcU");
            }
            payload_status
        } else {
            // Synthesise a syncing response to send, but do not cache it.
            tracing::info!(id = ?id, head_hash = ?head_hash, "sending SYNCING status on fcU");
            JsonPayloadStatusV1 {
                status: JsonPayloadStatusV1Status::Syncing,
                latest_valid_hash: None,
                validation_error: None,
            }
        };

        // FIXME: wait for payload attributes from controller?
        let payload_id = if let Some(payload_attributes) = opt_payload_attributes {
            match self
                .get_existing_payload_id(
                    head_hash,
                    JsonPayloadAttributes::V2(payload_attributes).into(),
                )
                .await
            {
                Ok(payload_id) => Some(TransparentJsonPayloadId(payload_id)),
                Err(message) => {
                    tracing::warn!(message, "unable to build payload for client");
                    None
                }
            }
        } else {
            None
        };

        let response = JsonForkchoiceUpdatedV1Response {
            payload_status,
            payload_id,
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
    ) -> Option<JsonPayloadStatusV1> {
        let mut cache = self.fcu_cache.lock().await;

        let existing_status = match self.config.fcu_matching {
            FcuMatching::Exact => cache.get(fcu),
            FcuMatching::Loose | FcuMatching::HeadOnly => {
                cache.iter().find_map(|(cached_fcu, res)| {
                    (cached_fcu.head_block_hash == fcu.head_block_hash).then_some(res)
                })
            }
        }?;
        let just_and_fin_ok = match self.config.fcu_matching {
            FcuMatching::Exact | FcuMatching::HeadOnly => true,
            FcuMatching::Loose => {
                self.justified_block_cache
                    .lock()
                    .await
                    .contains(&fcu.safe_block_hash)
                    && self
                        .finalized_block_cache
                        .lock()
                        .await
                        .contains(&fcu.finalized_block_hash)
            }
        };

        let definite_enough = !definite_only || Self::is_definite(existing_status);

        if just_and_fin_ok && definite_enough {
            Some(existing_status.clone())
        } else {
            None
        }
    }

    pub fn is_definite(status: &JsonPayloadStatusV1) -> bool {
        use JsonPayloadStatusV1Status::*;
        match status.status {
            Valid | Invalid | InvalidBlockHash => true,
            Accepted | Syncing => false,
        }
    }
}
