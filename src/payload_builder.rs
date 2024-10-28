use crate::{
    base_fee::expected_base_fee_per_gas,
    types::{
        JsonBlobsBundleV1, JsonExecutionPayload, JsonGetPayloadResponseV1,
        JsonGetPayloadResponseV2, JsonGetPayloadResponseV3, JsonPayloadStatusV1Status, PayloadId,
        TransparentJsonPayloadId,
    },
    ErrorResponse, Multiplexer, Request, Response,
};
use eth2::types::{
    BlobsBundle, EthSpec, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadBellatrix,
    ExecutionPayloadCapella, ExecutionPayloadDeneb, FixedVector, ForkName, Hash256, Uint256,
    Unsigned, VariableList,
};
use execution_layer::{calculate_execution_block_hash, PayloadAttributes};
use lru::LruCache;
use std::marker::PhantomData;
use std::num::NonZeroUsize;

/// Information about previously seen canonical payloads which is used for building descendant payloads.
#[derive(Debug, Clone, Copy)]
pub struct PayloadInfo {
    /// Execution block number.
    pub block_number: u64,
    /// Execution state root.
    ///
    /// We use this as the state root of the block built upon this block. For Bellatrix this allows
    /// us to build valid blocks, but post-Capella this doesn't work because the withdrawals
    /// affect the state root and we can't compute that change without an EL.
    pub state_root: Hash256,
    /// For EIP-1559 calculations.
    pub base_fee_per_gas: Uint256,
    pub gas_used: u64,
    pub gas_limit: u64,
}

pub struct PayloadBuilder<E: EthSpec> {
    next_payload_id: u64,
    payload_attributes: LruCache<(ExecutionBlockHash, PayloadAttributes), PayloadId>,
    /// Map from block hash to information about canonical, non-dummy payloads.
    payload_info: LruCache<ExecutionBlockHash, PayloadInfo>,
    /// Map from payload ID to dummy execution payload.
    payloads: LruCache<PayloadId, ExecutionPayload<E>>,
    extra_data: VariableList<u8, E::MaxExtraDataBytes>,
    _phantom: PhantomData<E>,
}

impl<E: EthSpec> PayloadBuilder<E> {
    pub fn new(cache_size: NonZeroUsize, extra_data_str: &str) -> Self {
        let extra_data_bytes = extra_data_str.as_bytes();
        let len = std::cmp::min(extra_data_bytes.len(), E::MaxExtraDataBytes::to_usize());
        let extra_data = VariableList::new(extra_data_bytes[..len].to_vec()).unwrap();

        Self {
            next_payload_id: 0,
            payload_attributes: LruCache::new(cache_size),
            payload_info: LruCache::new(cache_size),
            payloads: LruCache::new(cache_size),
            extra_data,
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
        let parent_beacon_block_root = payload_attributes.parent_beacon_block_root().ok();

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
        let state_root = parent_info.state_root;
        let receipts_root = keccak_hash::KECCAK_EMPTY_LIST_RLP.as_fixed_bytes().into();
        let logs_bloom = FixedVector::default();
        let gas_used = 0;
        let extra_data = builder.extra_data.clone();
        let base_fee_per_gas = expected_base_fee_per_gas(
            parent_info.base_fee_per_gas,
            parent_info.gas_used,
            parent_info.gas_limit,
        );
        let blob_gas_used = 0;
        let excess_blob_gas = 0;
        let block_hash = ExecutionBlockHash::zero();

        let mut payload = match fork_name {
            ForkName::Bellatrix => ExecutionPayload::Bellatrix(ExecutionPayloadBellatrix {
                parent_hash,
                fee_recipient,
                state_root,
                receipts_root,
                logs_bloom,
                prev_randao,
                block_number,
                gas_limit,
                gas_used,
                timestamp,
                extra_data,
                base_fee_per_gas,
                block_hash,
                transactions,
            }),
            ForkName::Capella => {
                let withdrawals = payload_attributes
                    .withdrawals()
                    .map_err(|_| "no withdrawals".to_string())?
                    .clone()
                    .into();
                ExecutionPayload::Capella(ExecutionPayloadCapella {
                    parent_hash,
                    fee_recipient,
                    state_root,
                    receipts_root,
                    logs_bloom,
                    prev_randao,
                    block_number,
                    gas_limit,
                    gas_used,
                    timestamp,
                    extra_data,
                    base_fee_per_gas,
                    block_hash,
                    transactions,
                    withdrawals,
                })
            }
            ForkName::Deneb => {
                let withdrawals = payload_attributes
                    .withdrawals()
                    .map_err(|_| "no withdrawals".to_string())?
                    .clone()
                    .into();
                ExecutionPayload::Deneb(ExecutionPayloadDeneb {
                    parent_hash,
                    fee_recipient,
                    state_root,
                    receipts_root,
                    logs_bloom,
                    prev_randao,
                    block_number,
                    gas_limit,
                    gas_used,
                    timestamp,
                    extra_data,
                    base_fee_per_gas,
                    block_hash,
                    transactions,
                    withdrawals,
                    blob_gas_used,
                    excess_blob_gas,
                })
            }
            // TODO: support Electra
            ForkName::Electra => todo!(),
            ForkName::Base | ForkName::Altair => return Err(format!("invalid fork: {fork_name}")),
        };

        let (block_hash, _) =
            calculate_execution_block_hash(payload.to_ref(), parent_beacon_block_root);
        *payload.block_hash_mut() = block_hash;

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
                state_root: payload.state_root(),
                base_fee_per_gas: payload.base_fee_per_gas(),
                gas_used: payload.gas_used(),
                gas_limit: payload.gas_limit(),
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
        let block_value = Uint256::ZERO;
        match json_payload {
            JsonExecutionPayload::V1(execution_payload) => Response::new(
                id,
                JsonGetPayloadResponseV1 {
                    execution_payload,
                    block_value,
                },
            ),
            JsonExecutionPayload::V2(execution_payload) => Response::new(
                id,
                JsonGetPayloadResponseV2 {
                    execution_payload,
                    block_value,
                },
            ),
            JsonExecutionPayload::V3(execution_payload) => {
                let blobs_bundle = JsonBlobsBundleV1::from(BlobsBundle::default());
                let should_override_builder = false;
                Response::new(
                    id,
                    JsonGetPayloadResponseV3 {
                        execution_payload,
                        block_value,
                        blobs_bundle,
                        should_override_builder,
                    },
                )
            }
            // TODO: Electra support
            JsonExecutionPayload::V4(_) => {
                todo!("Electra")
            }
        }
    }
}
