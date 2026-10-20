pub mod candle_gaps;
pub mod history;
pub mod ingest;
pub mod liquidity;
pub mod orders;
pub mod reconstruction;
pub mod rpc;
pub mod tick_ledger;
pub mod transaction;
/// Deterministic, offline primitives. No RPC requests or wallet signing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swap {
    pub timestamp_ms: u64,
    pub price_usd: f64,
    pub base_amount: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candle {
    pub start_ms: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume_base: f64,
    pub volume_quote: f64,
    pub trades: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketError {
    InvalidPrice,
    InvalidVolume,
    InvalidInterval,
    OutOfOrder,
    NonFinite,
    InsufficientFunds,
    InvalidFraction,
}
fn finite_positive(x: f64) -> bool {
    x.is_finite() && x > 0.0
}
pub fn aggregate(swaps: &[Swap], interval_ms: u64) -> Result<Vec<Candle>, MarketError> {
    if interval_ms == 0 {
        return Err(MarketError::InvalidInterval);
    }
    let mut candles: Vec<Candle> = Vec::new();
    let mut previous = None;
    for s in swaps {
        if !finite_positive(s.price_usd) {
            return Err(MarketError::InvalidPrice);
        }
        if !s.base_amount.is_finite() || s.base_amount <= 0.0 {
            return Err(MarketError::InvalidVolume);
        }
        if previous.is_some_and(|p| s.timestamp_ms < p) {
            return Err(MarketError::OutOfOrder);
        }
        previous = Some(s.timestamp_ms);
        let start = s.timestamp_ms / interval_ms * interval_ms;
        let q = s.price_usd * s.base_amount;
        if !q.is_finite() {
            return Err(MarketError::NonFinite);
        }
        if let Some(c) = candles.last_mut() {
            if c.start_ms == start {
                c.high = c.high.max(s.price_usd);
                c.low = c.low.min(s.price_usd);
                c.close = s.price_usd;
                c.volume_base += s.base_amount;
                c.volume_quote += q;
                c.trades += 1;
                if !c.volume_base.is_finite() || !c.volume_quote.is_finite() {
                    return Err(MarketError::NonFinite);
                }
                continue;
            }
        }
        candles.push(Candle {
            start_ms: start,
            open: s.price_usd,
            high: s.price_usd,
            low: s.price_usd,
            close: s.price_usd,
            volume_base: s.base_amount,
            volume_quote: q,
            trades: 1,
        });
    }
    Ok(candles)
}
pub const TIMEFRAMES_MS: [u64; 7] = [
    30_000, 60_000, 300_000, 900_000, 3_600_000, 14_400_000, 86_400_000,
];
pub fn fdv_usd(supply_ui: f64, price_usd: f64) -> Result<f64, MarketError> {
    if !supply_ui.is_finite() || supply_ui < 0.0 {
        return Err(MarketError::InvalidVolume);
    }
    if !finite_positive(price_usd) {
        return Err(MarketError::InvalidPrice);
    }
    let value = supply_ui * price_usd;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MarketError::NonFinite)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperAccount {
    pub cash_usd: f64,
    pub base_qty: f64,
    pub avg_entry_usd: f64,
    pub realized_pnl_usd: f64,
}
impl PaperAccount {
    pub fn new(cash_usd: f64) -> Result<Self, MarketError> {
        if !cash_usd.is_finite() || cash_usd < 0.0 {
            return Err(MarketError::InsufficientFunds);
        }
        Ok(Self {
            cash_usd,
            base_qty: 0.0,
            avg_entry_usd: 0.0,
            realized_pnl_usd: 0.0,
        })
    }
    pub fn buy(&mut self, fraction: f64, price: f64, fee_rate: f64) -> Result<f64, MarketError> {
        if !finite_positive(price) {
            return Err(MarketError::InvalidPrice);
        }
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return Err(MarketError::InvalidFraction);
        }
        if !fee_rate.is_finite() || !(0.0..1.0).contains(&fee_rate) {
            return Err(MarketError::InvalidFraction);
        }
        let spend = self.cash_usd * fraction;
        let qty = spend / (price * (1.0 + fee_rate));
        if !qty.is_finite() {
            return Err(MarketError::NonFinite);
        }
        if qty == 0.0 {
            return Ok(0.0);
        }
        let old_cost = self.avg_entry_usd * self.base_qty;
        let new_cost = old_cost + spend;
        let new_qty = self.base_qty + qty;
        if !new_cost.is_finite() || !new_qty.is_finite() {
            return Err(MarketError::NonFinite);
        }
        self.cash_usd -= spend;
        self.base_qty = new_qty;
        self.avg_entry_usd = new_cost / new_qty;
        Ok(qty)
    }
    pub fn sell(&mut self, fraction: f64, price: f64, fee_rate: f64) -> Result<f64, MarketError> {
        if !finite_positive(price) {
            return Err(MarketError::InvalidPrice);
        }
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return Err(MarketError::InvalidFraction);
        }
        if !fee_rate.is_finite() || !(0.0..1.0).contains(&fee_rate) {
            return Err(MarketError::InvalidFraction);
        }
        let qty = self.base_qty * fraction;
        let proceeds = qty * price * (1.0 - fee_rate);
        let cost = qty * self.avg_entry_usd;
        if !proceeds.is_finite() || !cost.is_finite() {
            return Err(MarketError::NonFinite);
        }
        self.cash_usd += proceeds;
        self.base_qty -= qty;
        self.realized_pnl_usd += proceeds - cost;
        if self.base_qty.abs() < 1e-12 {
            self.base_qty = 0.0;
            self.avg_entry_usd = 0.0;
        }
        Ok(proceeds)
    }
    pub fn equity(&self, price: f64) -> Result<f64, MarketError> {
        if !finite_positive(price) {
            return Err(MarketError::InvalidPrice);
        }
        Ok(self.cash_usd + self.base_qty * price)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_to_one_second_to_ten_minute_parity() {
        let ticks = [
            Swap {
                timestamp_ms: 1_005,
                price_usd: 2.0,
                base_amount: 3.0,
            },
            Swap {
                timestamp_ms: 1_127,
                price_usd: 2.5,
                base_amount: 4.0,
            },
            Swap {
                timestamp_ms: 2_001,
                price_usd: 1.9,
                base_amount: 2.0,
            },
            Swap {
                timestamp_ms: 599_990,
                price_usd: 3.0,
                base_amount: 1.0,
            },
        ];
        let seconds = aggregate(&ticks, 1_000).unwrap();
        assert_eq!(seconds.len(), 3);
        assert_eq!(seconds[0].open, 2.0);
        assert_eq!(seconds[0].high, 2.5);
        assert_eq!(seconds[0].trades, 2);
        assert_eq!(seconds[0].volume_base, 7.0);
        let ten_min = aggregate(&ticks, 600_000).unwrap();
        assert_eq!(ten_min.len(), 1);
        assert_eq!(ten_min[0].trades, 4);
        assert_eq!(ten_min[0].open, 2.0);
        assert_eq!(ten_min[0].close, 3.0);
        assert_eq!(ten_min[0].low, 1.9);
        assert_eq!(ten_min[0].high, 3.0);
        assert_eq!(
            ten_min[0].volume_base,
            seconds.iter().map(|c| c.volume_base).sum::<f64>()
        );
        assert_eq!(
            ten_min[0].volume_quote,
            seconds.iter().map(|c| c.volume_quote).sum::<f64>()
        );
    }
    #[test]
    fn bars_align_and_are_true_ohlcv() {
        let s = [
            Swap {
                timestamp_ms: 30_001,
                price_usd: 2.0,
                base_amount: 2.0,
            },
            Swap {
                timestamp_ms: 39_000,
                price_usd: 3.0,
                base_amount: 1.0,
            },
            Swap {
                timestamp_ms: 60_000,
                price_usd: 1.0,
                base_amount: 3.0,
            },
        ];
        let c = aggregate(&s, 30_000).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].start_ms, 30_000);
        assert_eq!(c[0].open, 2.0);
        assert_eq!(c[0].high, 3.0);
        assert_eq!(c[0].low, 2.0);
        assert_eq!(c[0].close, 3.0);
        assert_eq!(c[0].volume_quote, 7.0);
        assert_eq!(c[1].start_ms, 60_000);
    }
    #[test]
    fn no_fake_empty_bars() {
        assert!(aggregate(&[], 30_000).unwrap().is_empty())
    }
    #[test]
    fn rejects_out_of_order() {
        let s = [
            Swap {
                timestamp_ms: 9,
                price_usd: 1.0,
                base_amount: 1.0,
            },
            Swap {
                timestamp_ms: 8,
                price_usd: 1.0,
                base_amount: 1.0,
            },
        ];
        assert_eq!(aggregate(&s, 30_000), Err(MarketError::OutOfOrder))
    }
    #[test]
    fn invalid_swaps_fail_closed() {
        assert_eq!(
            aggregate(
                &[Swap {
                    timestamp_ms: 0,
                    price_usd: f64::NAN,
                    base_amount: 1.0
                }],
                30_000
            ),
            Err(MarketError::InvalidPrice)
        )
    }
    #[test]
    fn timeframes_are_explicit() {
        assert_eq!(
            TIMEFRAMES_MS,
            [30_000, 60_000, 300_000, 900_000, 3_600_000, 14_400_000, 86_400_000]
        )
    }
    #[test]
    fn fdv_checked() {
        assert_eq!(fdv_usd(100.0, 2.5), Ok(250.0));
        assert!(fdv_usd(f64::INFINITY, 2.0).is_err())
    }
    #[test]
    fn paper_round_trip_accounts_for_fees() {
        let mut a = PaperAccount::new(100.0).unwrap();
        let q = a.buy(1.0, 10.0, 0.01).unwrap();
        let _ = a.sell(1.0, 10.0, 0.01).unwrap();
        assert!(q > 9.9);
        assert!(a.cash_usd < 100.0);
        assert!(a.realized_pnl_usd < 0.0);
        assert_eq!(a.base_qty, 0.0)
    }
    #[test]
    fn position_size_zero_is_noop() {
        let mut a = PaperAccount::new(100.0).unwrap();
        assert_eq!(a.buy(0.0, 2.0, 0.01), Ok(0.0));
        assert_eq!(a.cash_usd, 100.0)
    }
    #[test]
    fn denies_fraction_above_one() {
        let mut a = PaperAccount::new(100.0).unwrap();
        assert_eq!(a.buy(1.01, 2.0, 0.01), Err(MarketError::InvalidFraction))
    }
}
