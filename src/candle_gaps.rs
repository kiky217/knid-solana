//! Explicit optional zero-volume candle gap filling for chart rendering.
//! Synthetic candles are not trades and must never be treated as market data evidence.
use crate::{Candle, MarketError};

/// Fills *interior* missing intervals only. Never extrapolates beyond observed data.
/// Bounded to prevent untrusted timestamps from causing unbounded allocations.
pub fn fill_interior_gaps(
    candles: &[Candle],
    interval_ms: u64,
    max_insertions: usize,
) -> Result<Vec<Candle>, MarketError> {
    if interval_ms == 0 {
        return Err(MarketError::InvalidInterval);
    }
    let mut result = Vec::with_capacity(candles.len());
    let mut inserted = 0_usize;
    for candle in candles {
        if candle.start_ms % interval_ms != 0 || !candle.close.is_finite() || candle.close <= 0.0 {
            return Err(MarketError::InvalidInterval);
        }
        if let Some(prev) = result.last().copied() {
            let prev: Candle = prev;
            if candle.start_ms <= prev.start_ms {
                return Err(MarketError::OutOfOrder);
            }
            let interval_count = (candle.start_ms - prev.start_ms) / interval_ms;
            let missing =
                usize::try_from(interval_count - 1).map_err(|_| MarketError::InvalidInterval)?;
            inserted = inserted
                .checked_add(missing)
                .ok_or(MarketError::InvalidInterval)?;
            if inserted > max_insertions {
                return Err(MarketError::InvalidInterval);
            }
            for index in 1..=missing {
                let timestamp = prev
                    .start_ms
                    .checked_add(
                        interval_ms
                            .checked_mul(index as u64)
                            .ok_or(MarketError::InvalidInterval)?,
                    )
                    .ok_or(MarketError::InvalidInterval)?;
                result.push(Candle {
                    start_ms: timestamp,
                    open: prev.close,
                    high: prev.close,
                    low: prev.close,
                    close: prev.close,
                    volume_base: 0.0,
                    volume_quote: 0.0,
                    trades: 0,
                });
            }
        }
        result.push(*candle);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bar(time: u64, price: f64) -> Candle {
        Candle {
            start_ms: time,
            open: price,
            high: price,
            low: price,
            close: price,
            volume_base: 1.0,
            volume_quote: price,
            trades: 1,
        }
    }
    #[test]
    fn fills_only_interior_without_trades() {
        let actual = [bar(0, 2.0), bar(90_000, 5.0)];
        let filled = fill_interior_gaps(&actual, 30_000, 2).unwrap();
        assert_eq!(filled.len(), 4);
        assert_eq!(filled[1].start_ms, 30_000);
        assert_eq!(filled[2].start_ms, 60_000);
        assert_eq!(filled[1].close, 2.0);
        assert_eq!(filled[2].volume_base, 0.0);
        assert_eq!(filled[2].volume_quote, 0.0);
        assert_eq!(filled[2].trades, 0);
        assert_eq!(filled[3], actual[1]);
    }
    #[test]
    fn rejects_unbounded_gap() {
        assert_eq!(
            fill_interior_gaps(&[bar(0, 1.0), bar(900_000, 2.0)], 30_000, 2),
            Err(MarketError::InvalidInterval)
        );
    }
    #[test]
    fn rejects_duplicate_and_unaligned_candles() {
        assert_eq!(
            fill_interior_gaps(&[bar(0, 1.0), bar(0, 2.0)], 30_000, 4),
            Err(MarketError::OutOfOrder)
        );
        assert_eq!(
            fill_interior_gaps(&[bar(1, 1.0)], 30_000, 4),
            Err(MarketError::InvalidInterval)
        );
    }
    #[test]
    fn empty_and_no_gaps() {
        assert!(fill_interior_gaps(&[], 30_000, 0).unwrap().is_empty());
        assert_eq!(
            fill_interior_gaps(&[bar(30_000, 1.0), bar(60_000, 2.0)], 30_000, 0)
                .unwrap()
                .len(),
            2
        );
    }
}
