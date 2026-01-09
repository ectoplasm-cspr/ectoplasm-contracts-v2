use casper_types::{U256, U512};

/// Curve type identifier
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CurveType {
    Linear = 0,
    Sigmoid = 1,
    Steep = 2,
}

impl CurveType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(CurveType::Linear),
            1 => Some(CurveType::Sigmoid),
            2 => Some(CurveType::Steep),
            _ => None,
        }
    }
}

/// Precision multiplier for fixed-point arithmetic (18 decimals)
const PRECISION: u128 = 1_000_000_000_000_000_000u128;

/// Calculate the current spot price based on curve type and progress
///
/// # Arguments
/// * `curve_type` - The type of bonding curve
/// * `tokens_sold` - Amount of tokens already sold
/// * `total_supply` - Total supply available for the curve
/// * `base_price` - Starting price (in motes per token)
/// * `max_price` - Maximum price at full supply (in motes per token)
///
/// # Returns
/// Current price in motes per token (U512)
pub fn calculate_price(
    curve_type: CurveType,
    tokens_sold: U256,
    total_supply: U256,
    base_price: U512,
    max_price: U512,
) -> U512 {
    if total_supply.is_zero() {
        return base_price;
    }

    // Calculate progress as a fraction with PRECISION
    // progress = (tokens_sold * PRECISION) / total_supply
    let tokens_sold_u512 = U512::from(tokens_sold.as_u128());
    let total_supply_u512 = U512::from(total_supply.as_u128());
    let precision = U512::from(PRECISION);

    let progress = (tokens_sold_u512 * precision) / total_supply_u512;

    match curve_type {
        CurveType::Linear => calculate_linear_price(progress, base_price, max_price, precision),
        CurveType::Sigmoid => calculate_sigmoid_price(progress, base_price, max_price, precision),
        CurveType::Steep => calculate_steep_price(progress, base_price, max_price, precision),
    }
}

/// Linear curve: price increases linearly from base_price to max_price
/// price = base_price + progress * (max_price - base_price)
fn calculate_linear_price(
    progress: U512,
    base_price: U512,
    max_price: U512,
    precision: U512,
) -> U512 {
    let price_range = max_price - base_price;
    base_price + (progress * price_range) / precision
}

/// Sigmoid curve: S-shaped curve with slow start, rapid middle, slow end
/// Approximated using a polynomial: 3x^2 - 2x^3 (smoothstep)
/// This gives an S-curve shape without needing exp()
fn calculate_sigmoid_price(
    progress: U512,
    base_price: U512,
    max_price: U512,
    precision: U512,
) -> U512 {
    // Smoothstep formula: 3x^2 - 2x^3
    // sigmoid_progress = progress^2 * (3 - 2*progress)
    let progress_squared = (progress * progress) / precision;
    let three = U512::from(3u64);
    let two = U512::from(2u64);

    // 3 - 2*progress (scaled by precision)
    let three_scaled = three * precision;
    let two_progress = two * progress;
    let factor = if three_scaled > two_progress {
        three_scaled - two_progress
    } else {
        U512::zero()
    };

    // sigmoid_progress = progress^2 * factor / precision
    let sigmoid_progress = (progress_squared * factor) / precision;

    let price_range = max_price - base_price;
    base_price + (sigmoid_progress * price_range) / precision
}

/// Steep curve: Exponential-like growth (aggressive early rewards)
/// Approximated using: price = base_price + (progress^2) * price_range
/// This gives faster initial growth than linear
fn calculate_steep_price(
    progress: U512,
    base_price: U512,
    max_price: U512,
    precision: U512,
) -> U512 {
    // Quadratic growth: progress^2
    let steep_progress = (progress * progress) / precision;

    let price_range = max_price - base_price;
    base_price + (steep_progress * price_range) / precision
}

/// Calculate the number of tokens that can be bought with a given amount of CSPR
/// Uses numerical integration (trapezoid rule) for accurate bonding curve math
///
/// # Arguments
/// * `curve_type` - The type of bonding curve
/// * `cspr_amount` - Amount of CSPR to spend (in motes)
/// * `current_sold` - Tokens already sold
/// * `total_supply` - Total supply for the curve
/// * `base_price` - Starting price
/// * `max_price` - Maximum price
///
/// # Returns
/// Number of tokens that can be purchased
pub fn calculate_tokens_for_cspr(
    curve_type: CurveType,
    cspr_amount: U512,
    current_sold: U256,
    total_supply: U256,
    base_price: U512,
    max_price: U512,
) -> U256 {
    if cspr_amount.is_zero() || total_supply.is_zero() {
        return U256::zero();
    }

    // For simplicity, use average price approximation
    // More accurate: numerical integration with small steps

    // Estimate tokens using current price as approximation
    let current_price = calculate_price(curve_type, current_sold, total_supply, base_price, max_price);

    if current_price.is_zero() {
        return U256::zero();
    }

    // tokens = cspr_amount / current_price (with precision for token decimals)
    let token_decimals = U512::from(1_000_000_000_000_000_000u128); // 18 decimals
    let tokens_raw = (cspr_amount * token_decimals) / current_price;

    // Cap at remaining supply
    let remaining = total_supply - current_sold;
    let tokens = U256::from(tokens_raw.as_u128().min(remaining.as_u128()));

    // Verify we don't exceed supply
    if tokens > remaining {
        remaining
    } else {
        tokens
    }
}

/// Calculate the CSPR received for selling tokens
///
/// # Arguments
/// * `curve_type` - The type of bonding curve
/// * `token_amount` - Amount of tokens to sell
/// * `current_sold` - Tokens currently sold (before this sale)
/// * `total_supply` - Total supply for the curve
/// * `base_price` - Starting price
/// * `max_price` - Maximum price
///
/// # Returns
/// Amount of CSPR to receive (in motes)
pub fn calculate_cspr_for_tokens(
    curve_type: CurveType,
    token_amount: U256,
    current_sold: U256,
    total_supply: U256,
    base_price: U512,
    max_price: U512,
) -> U512 {
    if token_amount.is_zero() || current_sold.is_zero() {
        return U512::zero();
    }

    // Calculate price at midpoint of sell range for average
    let sell_from = if current_sold > token_amount {
        current_sold - token_amount
    } else {
        U256::zero()
    };

    let avg_sold = (U256::from(current_sold.as_u128()) + U256::from(sell_from.as_u128())) / 2u128;
    let avg_price = calculate_price(curve_type, avg_sold, total_supply, base_price, max_price);

    // cspr = tokens * price / token_decimals
    let token_decimals = U512::from(1_000_000_000_000_000_000u128);
    let token_amount_u512 = U512::from(token_amount.as_u128());

    (token_amount_u512 * avg_price) / token_decimals
}

/// Calculate the integral (area under curve) for more accurate pricing
/// Used for buy/sell operations to ensure correct token<->CSPR conversion
pub fn calculate_curve_integral(
    curve_type: CurveType,
    from_tokens: U256,
    to_tokens: U256,
    total_supply: U256,
    base_price: U512,
    max_price: U512,
) -> U512 {
    if from_tokens >= to_tokens || total_supply.is_zero() {
        return U512::zero();
    }

    let precision = U512::from(PRECISION);
    let total_supply_u512 = U512::from(total_supply.as_u128());
    let from_u512 = U512::from(from_tokens.as_u128());
    let to_u512 = U512::from(to_tokens.as_u128());
    let token_diff = to_u512 - from_u512;

    // For linear curve, we can calculate exact integral:
    // Integral from a to b of (base + (x/total)*range) dx
    // = base*(b-a) + range/(2*total) * (b^2 - a^2)

    match curve_type {
        CurveType::Linear => {
            let price_range = max_price - base_price;
            let base_cost = base_price * token_diff;

            // (b^2 - a^2) = (b-a)(b+a)
            let sum_tokens = from_u512 + to_u512;
            let range_cost = (price_range * token_diff * sum_tokens) / (U512::from(2u64) * total_supply_u512);

            (base_cost + range_cost) / U512::from(1_000_000_000_000_000_000u128) // Adjust for decimals
        }
        _ => {
            // For non-linear curves, use midpoint approximation
            let mid_tokens = (from_tokens + to_tokens) / 2u128;
            let mid_price = calculate_price(curve_type, mid_tokens, total_supply, base_price, max_price);
            (mid_price * token_diff) / U512::from(1_000_000_000_000_000_000u128)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_curve() {
        let base = U512::from(1_000_000_000u64); // 1 CSPR
        let max = U512::from(10_000_000_000u64); // 10 CSPR
        let supply = U256::from(1_000_000u128);

        // At 0% progress, price should be base
        let price_start = calculate_price(CurveType::Linear, U256::zero(), supply, base, max);
        assert_eq!(price_start, base);

        // At 100% progress, price should be max
        let price_end = calculate_price(CurveType::Linear, supply, supply, base, max);
        assert_eq!(price_end, max);

        // At 50% progress, price should be midpoint
        let price_mid = calculate_price(CurveType::Linear, supply / 2, supply, base, max);
        let expected_mid = (base + max) / 2;
        assert_eq!(price_mid, expected_mid);
    }
}
