use crate::{
    types::{JsonExecutionPayload, JsonPayloadStatusV1Status, PayloadId, TransparentJsonPayloadId},
    ErrorResponse, Multiplexer, Request, Response,
};
use eth2::types::{
    EthSpec, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadCapella, ExecutionPayloadMerge,
    ForkName, VariableList,
};
use execution_layer::PayloadAttributes;
use lru::LruCache;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

/// Information about previously seen canonical payloads which is used for building descendant payloads.
#[derive(Debug, Clone, Copy)]
pub struct PayloadInfo {
    pub block_number: u64,
}

pub struct PayloadBuilder<E: EthSpec> {
    next_payload_id: u64,
    payload_attributes: LruCache<(ExecutionBlockHash, PayloadAttributes), PayloadId>,
    /// Map from block hash to information about canonical, non-dummy payloads.
    payload_info: LruCache<ExecutionBlockHash, PayloadInfo>,
    /// Map from payload ID to dummy execution payload.
    payloads: LruCache<PayloadId, ExecutionPayload<E>>,
    _phantom: PhantomData<E>,
}

impl<E: EthSpec> PayloadBuilder<E> {
    pub fn new(cache_size: NonZeroUsize) -> Self {
        Self {
            next_payload_id: 0,
            payload_attributes: LruCache::new(cache_size),
            payload_info: LruCache::new(cache_size),
            payloads: LruCache::new(cache_size),
            _phantom: PhantomData,
        }
    }
}

impl<E: EthSpec> Multiplexer<E> {
    pub async fn register_attributes(
        &self,
        parent_hash: ExecutionBlockHash,
        payload_attributes: PayloadAttributes,
    ) -> Result<PayloadId, String> {
        let timestamp = payload_attributes.timestamp();
        let Some(slot) = self.timestamp_to_slot(timestamp) else {
            return Err(format!("invalid timestamp {timestamp}"));
        };

        let mut builder = self.payload_builder.lock().await;
        let attributes_key = (parent_hash, payload_attributes);
        let payload_attributes = &attributes_key.1;

        // Return early if payload already known/built.
        if let Some(id) = builder.payload_attributes.get(&attributes_key) {
            return Ok(*id);
        }

        // Check that the head block is known.
        let Some(parent_info) = builder.payload_info.get(&parent_hash).copied() else {
            return Err(format!("unknown parent: {parent_hash:?}"));
        };

        // Allocate a payload ID.
        let id = builder.next_payload_id.to_be_bytes();

        // Build.
        let block_number = parent_info.block_number + 1;
        let fee_recipient = payload_attributes.suggested_fee_recipient();
        let prev_randao = payload_attributes.prev_randao();
        let gas_limit = 30_000_000;
        let fork_name = self.spec.fork_name_at_slot::<E>(slot);
        let transactions = VariableList::new(vec![]).unwrap();

        let payload = match fork_name {
            ForkName::Merge => ExecutionPayload::Merge(ExecutionPayloadMerge {
                parent_hash,
                timestamp,
                fee_recipient,
                prev_randao,
                block_number,
                gas_limit,
                transactions,
                ..Default::default()
            }),
            ForkName::Capella => {
                let withdrawals = payload_attributes
                    .withdrawals()
                    .map_err(|_| "no withdrawals".to_string())?
                    .clone()
                    .into();
                ExecutionPayload::Capella(ExecutionPayloadCapella {
                    parent_hash,
                    timestamp,
                    fee_recipient,
                    prev_randao,
                    block_number,
                    gas_limit,
                    transactions,
                    withdrawals,
                    ..Default::default()
                })
            }
            ForkName::Base | ForkName::Altair => return Err(format!("invalid fork: {fork_name}")),
        };

        builder.payload_attributes.put(attributes_key, id);
        builder.payloads.put(id, payload);
        builder.next_payload_id += 1;

        Ok(id)
    }

    pub async fn get_existing_payload_id(
        &self,
        parent_hash: ExecutionBlockHash,
        payload_attributes: PayloadAttributes,
    ) -> Result<PayloadId, String> {
        self.payload_builder
            .lock()
            .await
            .payload_attributes
            .get(&(parent_hash, payload_attributes))
            .copied()
            .ok_or_else(|| format!("no payload ID known for parent {parent_hash:?}"))
    }

    /// Track a payload from the canonical chain.
    pub async fn register_canonical_payload(
        &self,
        payload: &ExecutionPayload<E>,
        status: JsonPayloadStatusV1Status,
    ) {
        if status == JsonPayloadStatusV1Status::Invalid
            || status == JsonPayloadStatusV1Status::InvalidBlockHash
        {
            return;
        }

        self.payload_builder
            .lock()
            .await
            .payload_info
            .get_or_insert(payload.block_hash(), || PayloadInfo {
                block_number: payload.block_number(),
            });
    }

    pub async fn get_payload(&self, payload_id: PayloadId) -> Result<ExecutionPayload<E>, String> {
        self.payload_builder
            .lock()
            .await
            .payloads
            .get(&payload_id)
            .cloned()
            .ok_or_else(|| {
                let payload_num = u64::from_be_bytes(payload_id);
                format!("unknown payload ID: {payload_num}")
            })
    }

    pub async fn handle_get_payload(&self, request: Request) -> Result<Response, ErrorResponse> {
        let (id, (payload_id,)) = request.parse_as::<(TransparentJsonPayloadId,)>()?;
        let payload = match self.get_payload(payload_id.into()).await {
            Ok(payload) => payload,
            Err(message) => return Err(ErrorResponse::unknown_payload(id, message)),
        };
        let json_payload = JsonExecutionPayload::from(payload);
        Response::new(id, json_payload)
    }
}
