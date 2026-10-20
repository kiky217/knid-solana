//! Native SOL-denominated OHLCV from decoded Pump.fun trade events.
//! Timestamp granularity is seconds; no historical millisecond execution claim.
use crate::backfill::HistoricalTrade;
use crate::rpc::RpcError;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct SolCandle {
    pub start_s: i64,
    pub open_sol: f64,
    pub high_sol: f64,
    pub low_sol: f64,
    pub close_sol: f64,
    pub volume_lamports: u128,
    pub volume_token_raw: u128,
    pub trades: u64,
    pub open_close_order_verified: bool,
}
/// Empty intervals are absent. Source coverage (mint/pools) is tracked separately.
pub fn aggregate_sol(
    trades: &[HistoricalTrade],
    interval_s: i64,
) -> Result<Vec<SolCandle>, RpcError> {
    if interval_s <= 0 || interval_s > 86_400 {
        return Err(RpcError::InvalidResponse);
    }
    let mut keys = HashSet::new();
    let mut rows = Vec::with_capacity(trades.len());
    for t in trades {
        if t.event.timestamp_s <= 0
            || t.price_sol <= 0.0
            || !t.price_sol.is_finite()
            || t.event.sol_lamports == 0
            || t.event.token_raw == 0
            || !keys.insert((t.signature.clone(), t.event.event_index))
        {
            return Err(RpcError::InvalidResponse);
        }
        rows.push(t);
    }
    rows.sort_by(|a, b| {
        (
            a.event.timestamp_s,
            a.slot,
            &a.signature,
            a.event.event_index,
        )
            .cmp(&(
                b.event.timestamp_s,
                b.slot,
                &b.signature,
                b.event.event_index,
            ))
    });
    let mut candles: Vec<SolCandle> = Vec::new();
    let mut prior: Option<&HistoricalTrade> = None;
    for t in rows {
        let start_s = t.event.timestamp_s.div_euclid(interval_s) * interval_s;
        let same_slot_ambiguous = prior.is_some_and(|p| {
            p.slot == t.slot
                && p.event.timestamp_s == t.event.timestamp_s
                && p.signature != t.signature
        });
        if let Some(c) = candles.last_mut() {
            if c.start_s == start_s {
                c.high_sol = c.high_sol.max(t.price_sol);
                c.low_sol = c.low_sol.min(t.price_sol);
                c.close_sol = t.price_sol;
                c.volume_lamports = c
                    .volume_lamports
                    .checked_add(t.event.sol_lamports as u128)
                    .ok_or(RpcError::InvalidResponse)?;
                c.volume_token_raw = c
                    .volume_token_raw
                    .checked_add(t.event.token_raw as u128)
                    .ok_or(RpcError::InvalidResponse)?;
                c.trades += 1;
                if same_slot_ambiguous {
                    c.open_close_order_verified = false;
                }
                prior = Some(t);
                continue;
            }
        }
        candles.push(SolCandle {
            start_s,
            open_sol: t.price_sol,
            high_sol: t.price_sol,
            low_sol: t.price_sol,
            close_sol: t.price_sol,
            volume_lamports: t.event.sol_lamports as u128,
            volume_token_raw: t.event.token_raw as u128,
            trades: 1,
            open_close_order_verified: true,
        });
        prior = Some(t);
    }
    Ok(candles)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pumpfun_trade::PumpTradeEvent;
    fn trade(slot: u64, signature: &str, second: i64, price: f64) -> HistoricalTrade {
        HistoricalTrade {
            slot,
            signature: signature.into(),
            price_sol: price,
            event: PumpTradeEvent {
                mint: "mint".into(),
                sol_lamports: 100,
                token_raw: 50,
                is_buy: true,
                timestamp_s: second,
                event_index: 0,
            },
        }
    }
    #[test]
    fn one_second_thirty_second_ten_minute_consistency() {
        let t = [
            trade(3, "three", 631, 3.0),
            trade(1, "one", 600, 1.0),
            trade(2, "two", 600, 2.0),
            trade(4, "four", 1199, 4.0),
        ];
        let one = aggregate_sol(&t, 1).unwrap();
        let thirty = aggregate_sol(&t, 30).unwrap();
        let ten = aggregate_sol(&t, 600).unwrap();
        assert_eq!((one.len(), thirty.len(), ten.len()), (3, 3, 1));
        assert_eq!(ten[0].open_sol, 1.0);
        assert_eq!(ten[0].close_sol, 4.0);
        assert_eq!(ten[0].high_sol, 4.0);
        assert_eq!(ten[0].low_sol, 1.0);
        assert_eq!(ten[0].volume_lamports, 400);
        assert_eq!(one.iter().map(|x| x.trades).sum::<u64>(), ten[0].trades);
    }
    #[test]
    fn same_slot_cross_transaction_order_unknown() {
        let t = [trade(1, "sigb", 3, 2.0), trade(1, "siga", 3, 1.0)];
        let candles = aggregate_sol(&t, 1).unwrap();
        assert_eq!(candles.len(), 1);
        assert!(!candles[0].open_close_order_verified);
    }
    #[test]
    fn rejects_duplicate_trade_events() {
        let t = trade(1, "sig", 1, 1.0);
        assert!(aggregate_sol(&[t.clone(), t], 1).is_err());
    }
    #[test]
    fn empty_periods_not_fabricated() {
        assert!(aggregate_sol(&[], 1).unwrap().is_empty());
        assert!(aggregate_sol(&[], 0).is_err());
    }
}
