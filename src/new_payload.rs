//! Handler for new payload.
use crate::{
    multiplexer::Multiplexer,
    types::{
        ErrorResponse, JsonExecutionPayload, JsonPayloadStatusV1, JsonPayloadStatusV1Status,
        JsonValue, QuantityU64, Request, Response,
    },
};
use eth2::types::{EthSpec, ExecutionBlockHash, ForkName, Slot};
use execution_layer::http::ENGINE_NEW_PAYLOAD_V1;

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_controller_new_payload(
        &self,
        request: Request,
    ) -> Result<Response, ErrorResponse> {
        let (id, execution_payload) = self.decode_execution_payload(request)?;

        // TODO: verify block hash
        let block_hash = execution_payload.block_hash();

        let status = if let Some(status) = self.get_cached_payload_status(&block_hash, true).await {
            status
        } else {
            // Send payload to the real EL.
            match self.engine.api.new_payload(execution_payload.into()).await {
                Ok(status) => status.into(),
                Err(e) => {
                    // Return an error to the controlling CL.
                    // TODO: consider flag to return SYNCING here (after block hash verif).
                    tracing::warn!(error = ?e, "error during newPayload");
                    return Err(ErrorResponse::invalid_request(
                        id,
                        format!("payload verification failed: see eleel logs"),
                    ));
                }
            }
        };

        Response::new(id, status)
    }

    pub async fn handle_new_payload(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, execution_payload) = self.decode_execution_payload(request)?;

        // TODO: verify block hash
        let block_hash = execution_payload.block_hash();

        let status = if let Some(status) = self.get_cached_payload_status(&block_hash, false).await
        {
            status
        } else {
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
                    id.clone(),
                    format!("timestamp string missing"),
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
        if let Some(existing_status) = cache.get(&execution_block_hash) {
            if !definite_only || Self::is_definite(&existing_status) {
                return Some(existing_status.clone());
            }
        }
        None
    }
}
