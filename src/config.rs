use clap::Parser;
use eth2_network_config::Eth2NetworkConfig;
use std::str::FromStr;

#[derive(Debug, Clone, Parser)]
#[command(about = "Ethereum execution engine multiplexer")]
pub struct Config {
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
