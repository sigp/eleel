//! In-memory storage for caching payload statuses, fork choice updates, etc.
//!
//! We may cache more here in future (e.g. payload bodies for reconstruction).
use crate::{
    config::Config,
    payload_builder::PayloadBuilder,
    types::{Auth, Engine, JsonForkchoiceStateV1, JsonPayloadStatusV1, TaskExecutor},
};
use eth2::types::{ChainSpec, EthSpec, ExecutionBlockHash};
use execution_layer::HttpJsonRpc;
use lru::LruCache;
use slog::Logger;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::Mutex;

pub struct Multiplexer<E: EthSpec> {
    pub engine: Engine,
    pub fcu_cache: Mutex<LruCache<JsonForkchoiceStateV1, JsonPayloadStatusV1>>,
    pub new_payload_cache: Mutex<LruCache<ExecutionBlockHash, JsonPayloadStatusV1>>,
    pub justified_block_cache: Mutex<LruCache<ExecutionBlockHash, ()>>,
    pub finalized_block_cache: Mutex<LruCache<ExecutionBlockHash, ()>>,
    pub payload_builder: Mutex<PayloadBuilder<E>>,
    pub genesis_time: u64,
    pub spec: ChainSpec,
    pub config: Config,
    pub log: Logger,
    _phantom: PhantomData<E>,
}

impl<E: EthSpec> Multiplexer<E> {
    pub fn new(config: Config, executor: TaskExecutor, log: Logger) -> Result<Self, String> {
        let engine: Engine = {
            let jwt_secret_path = PathBuf::from(&config.ee_jwt_secret);
            let jwt_id = Some("eleel".to_string());
            let jwt_version = None;

            let execution_timeout_multiplier = Some(2);

            let auth = Auth::new_with_path(jwt_secret_path, jwt_id, jwt_version)
                .map_err(|e| format!("JWT secret error: {e:?}"))?;

            let url =
                FromStr::from_str(&config.ee_url).map_err(|e| format!("Invalid EL URL: {e:?}"))?;
            let api = HttpJsonRpc::new_with_auth(url, auth, execution_timeout_multiplier)
                .map_err(|e| format!("Error connecting to EL: {e:?}"))?;

            Engine::new(api, executor, &log)
        };

        let fcu_cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(config.fcu_cache_size).ok_or("invalid cache size")?,
        ));
        let new_payload_cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(config.new_payload_cache_size).ok_or("invalid cache size")?,
        ));
        let justified_block_cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(config.justified_block_cache_size).ok_or("invalid cache size")?,
        ));
        let finalized_block_cache = Mutex::new(LruCache::new(
            NonZeroUsize::new(config.justified_block_cache_size).ok_or("invalid cache size")?,
        ));
        let payload_builder = Mutex::new(PayloadBuilder::new(
            NonZeroUsize::new(config.payload_builder_cache_size).ok_or("invalid cache size")?,
        ));

        // Derived values.
        let spec = config.network.network.chain_spec::<E>()?;
        let genesis_state = config.network.network.beacon_state::<E>()?;
        let genesis_time = genesis_state.genesis_time();

        Ok(Self {
            engine,
            fcu_cache,
            new_payload_cache,
            justified_block_cache,
            finalized_block_cache,
            payload_builder,
            genesis_time,
            spec,
            config,
            log,
            _phantom: PhantomData,
        })
    }
}
