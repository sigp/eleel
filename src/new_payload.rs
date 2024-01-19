//! Handler for new payload.
use crate::{
    multiplexer::{Multiplexer, NewPayloadCacheEntry},
    types::{
        ErrorResponse, JsonExecutionPayload, JsonPayloadStatusV1, JsonPayloadStatusV1Status,
        JsonValue, NewPayloadRequest, NewPayloadRequestCapella, NewPayloadRequestDeneb,
        NewPayloadRequestMerge, QuantityU64, Request, Response,
    },
};
use eth2::types::{
    EthSpec, ExecutionBlockHash, ExecutionPayload, ForkName, Hash256, Slot, VersionedHash,
};
use execution_layer::{
    http::{ENGINE_NEW_PAYLOAD_V1, ENGINE_NEW_PAYLOAD_V2, ENGINE_NEW_PAYLOAD_V3},
    ExecutionLayer,
};
use std::time::{Duration, Instant};

impl<E: EthSpec> Multiplexer<E> {
    pub async fn handle_controller_new_payload(
        &self,
        request: Request,
    ) -> Result<Response, ErrorResponse> {
        let method = request.method.clone();
        tracing::info!(method = method, "processing payload from controller");
        let (id, json_execution_payload, versioned_hashes, parent_beacon_block_root) =
            self.decode_new_payload(request)?;

        let block_hash = *json_execution_payload.block_hash();
        let block_number = *json_execution_payload.block_number();

        let execution_payload = ExecutionPayload::from(json_execution_payload);
        let new_payload_request = match execution_payload.clone() {
            ExecutionPayload::Merge(execution_payload) => {
                NewPayloadRequest::Merge(NewPayloadRequestMerge { execution_payload })
            }
            ExecutionPayload::Capella(execution_payload) => {
                NewPayloadRequest::Capella(NewPayloadRequestCapella { execution_payload })
            }
            ExecutionPayload::Deneb(execution_payload) => {
                // TODO: error here if versioned hashes or parent root are None
                NewPayloadRequest::Deneb(NewPayloadRequestDeneb {
                    execution_payload,
                    versioned_hashes: versioned_hashes.unwrap_or_default(),
                    parent_beacon_block_root: parent_beacon_block_root.unwrap_or_default(),
                })
            }
        };

        let status = if let Some(status) = self.get_cached_payload_status(&block_hash, true).await {
            status
        } else {
            // Send payload to the real EL.
            match self.engine.api.new_payload(new_payload_request).await {
                Ok(status) => {
                    let json_status = JsonPayloadStatusV1::from(status);

                    // Update newPayload cache.
                    self.new_payload_cache.lock().await.put(
                        block_hash,
                        NewPayloadCacheEntry {
                            status: json_status.clone(),
                            block_number,
                        },
                    );

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
        // TODO: verify versioned hashes
        let (id, execution_payload, _versioned_hashes, parent_block_root) =
            self.decode_new_payload(request)?;

        // TODO: should check block hash validity before keying the cache on it
        let block_hash = *execution_payload.block_hash();
        let block_number = *execution_payload.block_number();

        // If this is a *recent* payload, wait a short time for a definite response from the EL.
        // Chances are it's busy processing the payload sent by the controlling BN.
        let is_recent = self.is_recent_payload(block_number).await;
        if is_recent {
            let start = Instant::now();
            while start.elapsed().as_millis() < self.config.new_payload_wait_millis {
                if let Some(status) = self.get_cached_payload_status(&block_hash, true).await {
                    return Response::new(id, status);
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        // Try again to get any status from the cache, or fall back on a SYNCING response.
        let status = if let Some(status) = self.get_cached_payload_status(&block_hash, false).await
        {
            if !Self::is_definite(&status) {
                tracing::info!("sending indefinite status on newPayload");
            }
            status
        } else {
            // Before sending a synthetic SYNCING response, check the block hash.
            // Use a 0x0 hash if no parent block root was provided. The hash is only required
            // for Deneb and later, and should be set (by decode_) whenever Deneb is activated.
            let execution_payload = ExecutionPayload::from(execution_payload);
            let (calculated_block_hash, _) = ExecutionLayer::<E>::calculate_execution_block_hash(
                execution_payload.to_ref(),
                parent_block_root.unwrap_or_default(),
            );

            if calculated_block_hash != block_hash {
                tracing::warn!(
                    expected = ?block_hash,
                    computed = ?calculated_block_hash,
                    "mismatched block hash"
                );
                return Err(ErrorResponse::invalid_request(
                    id,
                    format!("mismatched block hash {calculated_block_hash:?} vs {block_hash:?}"),
                ));
            }
            if is_recent {
                tracing::info!("sending SYNCING response on recent newPayload");
            } else {
                tracing::info!("sending instant SYNCING response for old newPayload");
            }
            // Synthetic syncing response.
            JsonPayloadStatusV1 {
                status: JsonPayloadStatusV1Status::Syncing,
                latest_valid_hash: None,
                validation_error: None,
            }
        };

        Response::new(id, status)
    }

    #[allow(clippy::type_complexity)]
    fn decode_new_payload(
        &self,
        request: Request,
    ) -> Result<
        (
            JsonValue,
            JsonExecutionPayload<E>,
            Option<Vec<VersionedHash>>,
            Option<Hash256>,
        ),
        ErrorResponse,
    > {
        let method = request.method.clone();

        let (id, params) = request.parse_as::<Vec<JsonValue>>()?;

        let (versioned_hashes, parent_block_root) = if method == ENGINE_NEW_PAYLOAD_V3 {
            if params.len() != 3 {
                return Err(ErrorResponse::parse_error_generic(
                    id,
                    "wrong number of parameters for newPayloadV3".to_string(),
                ));
            }
            let versioned_hashes = serde_json::from_value(params[1].clone())
                .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;
            let parent_block_root = serde_json::from_value(params[2].clone())
                .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;
            (Some(versioned_hashes), Some(parent_block_root))
        } else if params.len() == 1 {
            (None, None)
        } else {
            return Err(ErrorResponse::parse_error_generic(
                id,
                format!("wrong number of parameters for {method}: {}", params.len()),
            ));
        };

        let payload_json = params[0].clone();
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

        let payload = if method == ENGINE_NEW_PAYLOAD_V1 || fork_name == ForkName::Merge {
            serde_json::from_value(payload_json).map(JsonExecutionPayload::V1)
        } else if method == ENGINE_NEW_PAYLOAD_V2 || fork_name == ForkName::Capella {
            serde_json::from_value(payload_json).map(JsonExecutionPayload::V2)
        } else {
            serde_json::from_value(payload_json).map(JsonExecutionPayload::V3)
        }
        .map_err(|e| ErrorResponse::parse_error(id.clone(), e))?;

        Ok((id, payload, versioned_hashes, parent_block_root))
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
        if let Some(existing) = cache.get(execution_block_hash) {
            if !definite_only || Self::is_definite(&existing.status) {
                return Some(existing.status.clone());
            }
        }
        None
    }

    /// Return the highest `block_number` of any cached payload, or 0 if none is cached.
    ///
    /// This is useful for approximately time-based cutoffs & heuristics.
    pub async fn highest_cached_payload_number(&self) -> u64 {
        let cache = self.new_payload_cache.lock().await;
        cache
            .iter()
            .map(|(_, entry)| entry.block_number)
            .max()
            .unwrap_or(0)
    }

    /// Check if the given block number is recent based on the `highest_cached_payload_number`.
    pub async fn is_recent_payload(&self, block_number: u64) -> bool {
        let cutoff = self
            .highest_cached_payload_number()
            .await
            .saturating_sub(self.config.new_payload_wait_cutoff);
        block_number >= cutoff
    }
}
