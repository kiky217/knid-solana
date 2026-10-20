//! Deterministic tick ledger for individually verified DEX swap events.
//! Timestamp is block-time estimate in milliseconds, not transaction millisecond precision.
use crate::{aggregate, Candle, MarketError, Swap};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct VerifiedTick {
    pub signature: String,
    pub slot: u64,
    pub tx_index: u32,
    pub ix: u16,
    pub inner_ix: Option<u16>,
    pub pool: String,
    pub timestamp_ms: u64,
    pub price_usd: f64,
    pub base_amount: f64,
}
#[derive(Default)]
pub struct TickLedger {
    keys: HashSet<(String, u16, Option<u16>, String)>,
    ticks: Vec<VerifiedTick>,
}
impl TickLedger {
    pub fn insert(&mut self, tick: VerifiedTick) -> Result<bool, MarketError> {
        if tick.signature.is_empty()
            || tick.pool.is_empty()
            || !tick.price_usd.is_finite()
            || tick.price_usd <= 0.0
            || !tick.base_amount.is_finite()
            || tick.base_amount <= 0.0
        {
            return Err(MarketError::InvalidPrice);
        }
        let key = (
            tick.signature.clone(),
            tick.ix,
            tick.inner_ix,
            tick.pool.clone(),
        );
        if !self.keys.insert(key) {
            return Ok(false);
        }
        self.ticks.push(tick);
        Ok(true)
    }
    pub fn ordered(&self) -> Vec<&VerifiedTick> {
        let mut rows: Vec<_> = self.ticks.iter().collect();
        rows.sort_by(|a, b| {
            (a.slot, a.tx_index, a.ix, a.inner_ix, &a.pool)
                .cmp(&(b.slot, b.tx_index, b.ix, b.inner_ix, &b.pool))
        });
        rows
    }
    pub fn candles(&self, interval_ms: u64) -> Result<Vec<Candle>, MarketError> {
        let ordered = self.ordered();
        let mut swaps: Vec<Swap> = Vec::with_capacity(ordered.len());
        for tick in ordered {
            // Time can be approximated per block. Reject contradictory time ordering;
            // never reassign fabricated milliseconds to make data fit.
            if swaps
                .last()
                .is_some_and(|s| s.timestamp_ms > tick.timestamp_ms)
            {
                return Err(MarketError::OutOfOrder);
            }
            swaps.push(Swap {
                timestamp_ms: tick.timestamp_ms,
                price_usd: tick.price_usd,
                base_amount: tick.base_amount,
            });
        }
        aggregate(&swaps, interval_ms)
    }
    pub fn len(&self) -> usize {
        self.ticks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ticks.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tick(slot: u64, tx_index: u32, ix: u16, ms: u64, price: f64) -> VerifiedTick {
        VerifiedTick {
            signature: format!("sig-{slot}-{tx_index}"),
            slot,
            tx_index,
            ix,
            inner_ix: None,
            pool: "pool-1".into(),
            timestamp_ms: ms,
            price_usd: price,
            base_amount: 1.0,
        }
    }
    #[test]
    fn dedup_and_reorder_to_canonical() {
        let mut l = TickLedger::default();
        assert_eq!(l.insert(tick(3, 0, 0, 2000, 3.0)), Ok(true));
        assert_eq!(l.insert(tick(2, 0, 0, 1000, 2.0)), Ok(true));
        assert_eq!(l.insert(tick(3, 0, 0, 2000, 3.0)), Ok(false));
        let v = l.candles(1000).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].open, 2.0);
        assert_eq!(v[1].close, 3.0);
        assert_eq!(l.len(), 2);
    }
    #[test]
    fn ten_minutes_reconstruct_from_ticks() {
        let mut l = TickLedger::default();
        l.insert(tick(4, 0, 0, 599_000, 5.0)).unwrap();
        l.insert(tick(2, 0, 0, 1000, 2.0)).unwrap();
        l.insert(tick(3, 0, 0, 1000, 3.0)).unwrap();
        let seconds = l.candles(1000).unwrap();
        let ten = l.candles(600_000).unwrap();
        assert_eq!(ten[0].open, 2.0);
        assert_eq!(ten[0].close, 5.0);
        assert_eq!(ten[0].high, 5.0);
        assert_eq!(ten[0].trades, 3);
        assert_eq!(seconds.iter().map(|c| c.trades).sum::<u64>(), 3);
    }
    #[test]
    fn contradictory_block_time_fails_closed() {
        let mut l = TickLedger::default();
        l.insert(tick(2, 0, 0, 2000, 2.0)).unwrap();
        l.insert(tick(3, 0, 0, 1000, 3.0)).unwrap();
        assert_eq!(l.candles(1000), Err(MarketError::OutOfOrder));
    }
}

[executed on device: 6a59e52b5668 (5255b4bd-412b-4ac8-9b22-0b77cd4dee13)]