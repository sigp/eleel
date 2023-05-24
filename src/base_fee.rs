//! EIP-1559 base fee per gas calculations.
//!
//! Translation of Python spec from: https://eips.ethereum.org/EIPS/eip-1559
use eth2::types::Uint256;
use std::cmp::max;

const ELASTICITY_MULTIPLIER: u64 = 2;
const BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;

pub fn expected_base_fee_per_gas(
    parent_base_fee_per_gas: Uint256,
    parent_gas_used: u64,
    parent_gas_limit: u64,
) -> Uint256 {
    let parent_gas_target = parent_gas_limit / ELASTICITY_MULTIPLIER;

    if parent_gas_used == parent_gas_target {
        parent_base_fee_per_gas
    } else if parent_gas_used > parent_gas_target {
        let gas_used_delta = parent_gas_used.saturating_sub(parent_gas_target);
        let base_fee_per_gas_delta = max(
            parent_base_fee_per_gas * gas_used_delta
                / parent_gas_target
                / BASE_FEE_MAX_CHANGE_DENOMINATOR,
            Uint256::one(),
        );
        parent_base_fee_per_gas + base_fee_per_gas_delta
    } else {
        let gas_used_delta = parent_gas_target.saturating_sub(parent_gas_used);
        let base_fee_per_gas_delta = parent_base_fee_per_gas * gas_used_delta
            / parent_gas_target
            / BASE_FEE_MAX_CHANGE_DENOMINATOR;
        parent_base_fee_per_gas.saturating_sub(base_fee_per_gas_delta)
    }
}
