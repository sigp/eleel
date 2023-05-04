use clap::{builder::PossibleValue, Parser, ValueEnum};
use eth2_network_config::Eth2NetworkConfig;
use std::net::IpAddr;
use std::str::FromStr;
use strum::{EnumString, IntoStaticStr};

#[derive(Debug, Clone, Parser)]
#[command(about = "Ethereum execution engine multiplexer")]
pub struct Config {
    /// Listening address for the HTTP server.
    #[arg(long, value_name = "IP", default_value = "0.0.0.0")]
    pub listen_address: IpAddr,
    /// Listening port for the HTTP server.
    #[arg(long, value_name = "PORT", default_value = "8552")]
    pub listen_port: u16,
    /// Primary execution engine to be shared by connected consensus nodes.
    #[arg(long, value_name = "URL", default_value = "http://localhost:8551")]
    pub ee_url: String,
    /// Path to the JWT secret for the primary execution engine.
    #[arg(long, value_name = "PATH")]
    pub ee_jwt_secret: String,
    /// Number of recent newPayload messages to cache in memory.
    #[arg(long, value_name = "N", default_value = "64")]
    pub new_payload_cache_size: usize,
    /// Number of recent forkchoiceUpdated messages to cache in memory.
    #[arg(long, value_name = "N", default_value = "64")]
    pub fcu_cache_size: usize,
    /// Number of payload attributes and past payloads to cache in memory.
    #[arg(long, value_name = "N", default_value = "8")]
    pub payload_builder_cache_size: usize,
    /// Number of justified block hashes to cache in memory.
    #[arg(long, value_name = "N", default_value = "4")]
    pub justified_block_cache_size: usize,
    /// Number of finalized block hashes to cache in memory.
    #[arg(long, value_name = "N", default_value = "4")]
    pub finalized_block_cache_size: usize,
    /// Choose the type of matching to use before returning a VALID fcU message to a client.
    #[arg(long, value_name = "NAME", default_value = "loose", value_enum)]
    pub fcu_matching: FcuMatching,
    /// Network that the consensus and execution nodes are operating on.
    #[arg(long, value_name = "NAME", default_value = "mainnet")]
    pub network: Network,
    /// Maximum time that a consensus node should wait for a newPayload response from the cache.
    ///
    /// We expect that the controlling consensus node and primary execution node will take some
    /// time to process requests, and that requests from consensus nodes could arrive while this
    /// processing is on-going. Using a timeout of 0 will often result in a SYNCING response, which
    /// will put the consensus node into optimistic sync. Using a longer timeout will allow the
    /// definitive (VALID) response from the execution engine to be returned, more closely matching
    /// the behaviour of a full execution engine.
    #[arg(long, value_name = "MILLIS", default_value = "2000")]
    pub new_payload_wait_millis: u128,
    /// Maximum time that a consensus node should wait for a forkchoiceUpdated response from the
    /// cache.
    ///
    /// See the docs for `--new-payload-wait-millis` for the purpose of this timeout.
    #[arg(long, value_name = "MILLIS", default_value = "1000")]
    pub fcu_wait_millis: u128,
    /// Maximum size of JSON-RPC message to accept from any connected consensus node.
    #[arg(long, value_name = "MEGABYTES", default_value = "128")]
    pub body_limit_mb: usize,
}

#[derive(Debug, Clone)]
pub struct Network {
    pub network: Eth2NetworkConfig,
}

impl FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Eth2NetworkConfig::constant(s)?
            .map(|network| Network { network })
            .ok_or_else(|| format!("unknown network: {s}"))
    }
}

#[derive(EnumString, IntoStaticStr, Debug, Clone, Copy)]
#[strum(serialize_all = "kebab-case")]
pub enum FcuMatching {
    /// Client fcU must match a prior fcU from the controller *exactly*.
    Exact,
    /// Client fcU must reference head, justified and finalized blocks from prior controller calls,
    /// but do not necessarily have to match any single 3-tuple.
    ///
    /// This admits some innocuous things like head blocks with old justification/finalization,
    /// but also some weird stuff like justification==finalization, which Prysm v4.0.0 and lower
    /// will sometimes send (see: https://github.com/prysmaticlabs/prysm/issues/12195).
    Loose,
    /// Client fcU must match the head block only: justification and finalization are ignored.
    ///
    /// This is the most dangerous and is not recommended.
    HeadOnly,
}

impl ValueEnum for FcuMatching {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Exact, Self::Loose, Self::HeadOnly]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        let s: &'static str = self.into();
        let pv = match self {
            FcuMatching::Exact => {
                PossibleValue::new(s).help("match head/safe/finalized from controller exactly")
            }
            FcuMatching::Loose => {
                PossibleValue::new(s).help("match head and sanity check safe/finalized")
            }
            FcuMatching::HeadOnly => {
                PossibleValue::new(s).help("match head and ignore safe/finalized (dangerous)")
            }
        };
        Some(pv)
    }
}
