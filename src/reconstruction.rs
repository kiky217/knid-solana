//! Reusable candle reconstruction and cross-timeframe integrity checks.
use crate::{aggregate, Candle, MarketError, Swap};

/// Aggregate raw verified swaps without fabricating empty intervals.
pub type CandleReconstruction = (Vec<Candle>, Vec<Candle>, Vec<Candle>);
pub fn reconstruction(swaps: &[Swap]) -> Result<CandleReconstruction, MarketError> {
    let seconds = aggregate(swaps, 1_000)?;
    let thirty = aggregate(swaps, 30_000)?;
    let ten_minutes = aggregate(swaps, 600_000)?;
    for bars in [&seconds, &thirty, &ten_minutes] {
        let trades: u64 = bars.iter().map(|c| c.trades).sum();
        if trades != swaps.len() as u64 {
            return Err(MarketError::InvalidVolume);
        }
    }
    Ok((seconds, thirty, ten_minutes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tick_reconstruction_parity_with_gaps() {
        let ticks = [
            Swap {
                timestamp_ms: 0,
                price_usd: 0.2,
                base_amount: 10.0,
            },
            Swap {
                timestamp_ms: 500,
                price_usd: 0.3,
                base_amount: 20.0,
            },
            Swap {
                timestamp_ms: 31_000,
                price_usd: 0.1,
                base_amount: 30.0,
            },
            Swap {
                timestamp_ms: 599_999,
                price_usd: 0.4,
                base_amount: 40.0,
            },
            Swap {
                timestamp_ms: 600_000,
                price_usd: 0.5,
                base_amount: 50.0,
            },
        ];
        let (seconds, thirty, ten) = reconstruction(&ticks).unwrap();
        assert_eq!((seconds.len(), thirty.len(), ten.len()), (4, 4, 2));
        assert_eq!(ten[0].trades, 4);
        assert_eq!(ten[0].volume_base, 100.0);
        assert_eq!(ten[1].open, 0.5);
        assert_eq!(seconds[0].open, 0.2);
        assert_eq!(seconds[0].close, 0.3);
        assert_eq!(thirty[0].trades, 2);
        assert_eq!(seconds.iter().map(|c| c.trades).sum::<u64>(), 5);
        assert_eq!(thirty.iter().map(|c| c.trades).sum::<u64>(), 5);
    }
    #[test]
    fn no_trade_means_no_candle() {
        let (s, t, m) = reconstruction(&[]).unwrap();
        assert!(s.is_empty() && t.is_empty() && m.is_empty());
    }
}

[executed on device: 6a59e52b5668 (5255b4bd-412b-4ac8-9b22-0b77cd4dee13)]