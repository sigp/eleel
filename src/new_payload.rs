//! Handler for new payload.
use crate::{
    multiplexer::Multiplexer,
    types::{
        ErrorResponse, JsonExecutionPayload, JsonPayloadStatusV1, JsonPayloadStatusV1Status,
        JsonValue, QuantityU64, Request, Response,
    },
};
use eth2::types::{EthSpec, ExecutionBlockHash, ExecutionPayload, ForkName, Slot};
use execution_layer::http::ENGINE_NEW_PAYLOAD_V1;
use std::time::{Duration, Instant};

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_controller_new_payload(
        &self,
        request: Request,
    ) -> Result<Response, ErrorResponse> {
        tracing::info!("processing payload from controller");
        let (id, json_execution_payload) = self.decode_execution_payload(request)?;

        // TODO: verify block hash
        let block_hash = *json_execution_payload.block_hash();

        let status = if let Some(status) = self.get_cached_payload_status(&block_hash, true).await {
            status
        } else {
            // Send payload to the real EL.
            let execution_payload = ExecutionPayload::from(json_execution_payload);
            match self.engine.api.new_payload(execution_payload.clone()).await {
                Ok(status) => {
                    let json_status = JsonPayloadStatusV1::from(status);

                    // Update newPayload cache.
                    self.new_payload_cache
                        .lock()
                        .await
                        .put(block_hash, json_status.clone());

                    // Update payload builder.
                    self.register_canonical_payload(&execution_payload, json_status.status)
                        .await;

                    json_status
                }
                Err(e) => {
                    // Return an error to the controlling CL.
                    // TODO: consider flag to return SYNCING here (after block hash verif).
                    tracing::warn!(error = ?e, "error during newPayload");
                    return Err(ErrorResponse::invalid_request(
                        id,
                        "payload verification failed: see eleel logs".to_string(),
                    ));
                }
            }
        };

        Response::new(id, status)
    }

    pub async fn handle_new_payload(&self, request: Request) -> Result<Response, ErrorResponse> {
        tracing::info!("processing new payload from client");
        let (id, execution_payload) = self.decode_execution_payload(request)?;

        // TODO: verify block hash
        let block_hash = execution_payload.block_hash();

        // Wait a short time for a definite response from the EL. Chances are it's busy processing
        // the payload sent by the controlling BN.
        let start = Instant::now();
        while start.elapsed().as_millis() < self.config.new_payload_wait_millis {
            if let Some(status) = self.get_cached_payload_status(block_hash, true).await {
                return Response::new(id, status);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Try again to get any status from the cache, or fall back on a SYNCING response.
        let status = if let Some(status) = self.get_cached_payload_status(block_hash, false).await {
            if !Self::is_definite(&status) {
                tracing::info!("sending indefinite status on newPayload");
            }
            status
        } else {
            tracing::info!("sending SYNCING response on newPayload");
            // Synthetic syncing response.
            JsonPayloadStatusV1 {
                status: JsonPayloadStatusV1Status::Syncing,
                latest_valid_hash: None,
                validation_error: None,
            }
        };

        Response::new(id, status)
    }

    fn decode_execution_payload(
        &self,
        request: Request,
    ) -> Result<(JsonValue, JsonExecutionPayload<E>), ErrorResponse> {
        let method = request.method.clone();

        let (id, (payload_json,)) = request.parse_as::<(JsonValue,)>()?;

        let QuantityU64 { value: timestamp } =
            if let Some(timestamp_json) = payload_json.get("timestamp") {
                serde_json::from_value(timestamp_json.clone())
                    .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?
            } else {
                return Err(ErrorResponse::parse_error_generic(
                    id,
                    "timestamp value missing".to_string(),
                ));
            };

        let slot = self.timestamp_to_slot(timestamp).ok_or_else(|| {
            ErrorResponse::parse_error_generic(
                id.clone(),
                format!("invalid timestamp: {timestamp}"),
            )
        })?;

        let fork_name = self.spec.fork_name_at_slot::<E>(slot);

        // TODO: this could be more generic
        let payload = if method == ENGINE_NEW_PAYLOAD_V1 || fork_name == ForkName::Merge {
            serde_json::from_value(payload_json).map(JsonExecutionPayload::V1)
        } else {
            serde_json::from_value(payload_json).map(JsonExecutionPayload::V2)
        }
        .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;

        Ok((id, payload))
    }

    pub fn timestamp_to_slot(&self, timestamp: u64) -> Option<Slot> {
        timestamp
            .checked_sub(self.genesis_time)?
            .checked_div(self.spec.seconds_per_slot)
            .map(Slot::new)
    }

    pub async fn get_cached_payload_status(
        &self,
        execution_block_hash: &ExecutionBlockHash,
        definite_only: bool,
    ) -> Option<JsonPayloadStatusV1> {
        let mut cache = self.new_payload_cache.lock().await;
        if let Some(existing_status) = cache.get(execution_block_hash) {
            if !definite_only || Self::is_definite(existing_status) {
                return Some(existing_status.clone());
            }
        }
        None
    }
}
