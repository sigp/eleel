use eth2_network_config::Eth2NetworkConfig;

pub struct Config {
    pub el_url: String,
    pub jwt_secret_path: String,
    pub fcu_cache_size: usize,
    pub new_payload_cache_size: usize,
    pub network_config: Eth2NetworkConfig,
    pub new_payload_wait_millis: u128,
    pub fcu_wait_millis: u128,
    pub body_limit_mb: usize,
}
