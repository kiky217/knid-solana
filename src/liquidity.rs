//! Pool liquidity valuation from verified vault balances and independent USD quotes.
use crate::MarketError;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolVault {
    pub token_a_raw: u64,
    pub token_a_decimals: u8,
    pub token_a_usd: f64,
    pub token_b_raw: u64,
    pub token_b_decimals: u8,
    pub token_b_usd: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolValuation {
    pub token_a_ui: f64,
    pub token_b_ui: f64,
    pub tvl_usd: f64,
    pub implied_a_per_b: f64,
}
pub fn value_pool(p: PoolVault) -> Result<PoolValuation, MarketError> {
    if p.token_a_decimals > 18 || p.token_b_decimals > 18 {
        return Err(MarketError::InvalidVolume);
    }
    if !p.token_a_usd.is_finite()
        || p.token_a_usd <= 0.0
        || !p.token_b_usd.is_finite()
        || p.token_b_usd <= 0.0
    {
        return Err(MarketError::InvalidPrice);
    }
    let a = p.token_a_raw as f64 / 10_f64.powi(p.token_a_decimals as i32);
    let b = p.token_b_raw as f64 / 10_f64.powi(p.token_b_decimals as i32);
    if a <= 0.0 || b <= 0.0 {
        return Err(MarketError::InvalidVolume);
    }
    let total = a * p.token_a_usd + b * p.token_b_usd;
    let ratio = a / b;
    if !total.is_finite() || !ratio.is_finite() {
        return Err(MarketError::NonFinite);
    }
    Ok(PoolValuation {
        token_a_ui: a,
        token_b_ui: b,
        tvl_usd: total,
        implied_a_per_b: ratio,
    })
}
pub fn circulating_market_cap(
    circulating_supply_raw: u64,
    decimals: u8,
    price_usd: f64,
) -> Result<f64, MarketError> {
    if decimals > 18 {
        return Err(MarketError::InvalidVolume);
    }
    let ui = circulating_supply_raw as f64 / 10_f64.powi(decimals as i32);
    crate::fdv_usd(ui, price_usd)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correct_vault_decimal_scaling() {
        let p = PoolVault {
            token_a_raw: 1_000_000,
            token_a_decimals: 6,
            token_a_usd: 2.0,
            token_b_raw: 3_000_000_000,
            token_b_decimals: 9,
            token_b_usd: 10.0,
        };
        let v = value_pool(p).unwrap();
        assert_eq!(v.tvl_usd, 32.0);
        assert_eq!(v.token_a_ui, 1.0);
        assert_eq!(v.token_b_ui, 3.0)
    }
    #[test]
    fn rejects_negative_or_nonfinite_oracle() {
        let p = PoolVault {
            token_a_raw: 1,
            token_a_decimals: 0,
            token_a_usd: f64::NAN,
            token_b_raw: 1,
            token_b_decimals: 0,
            token_b_usd: 1.0,
        };
        assert_eq!(value_pool(p), Err(MarketError::InvalidPrice))
    }
    #[test]
    fn distinguishes_circulating_from_fdv() {
        assert_eq!(circulating_market_cap(20_000_000, 6, 2.0), Ok(40.0));
        assert_eq!(crate::fdv_usd(100.0, 2.0), Ok(200.0))
    }
    #[test]
    fn rejects_zero_reserves() {
        let p = PoolVault {
            token_a_raw: 0,
            token_a_decimals: 0,
            token_a_usd: 1.0,
            token_b_raw: 1,
            token_b_decimals: 0,
            token_b_usd: 1.0,
        };
        assert_eq!(value_pool(p), Err(MarketError::InvalidVolume))
    }
}
