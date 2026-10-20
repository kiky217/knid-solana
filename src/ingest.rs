//! Deduplicate decoded, independently verified DEX swaps before OHLCV aggregation.
use crate::{MarketError, Swap};
use std::collections::HashSet;
#[derive(Clone, Debug)]
pub struct VerifiedDexSwap {
    pub signature: String,
    pub slot: u64,
    pub ix: u16,
    pub inner_ix: Option<u16>,
    pub program: String,
    pub finalized: bool,
    pub timestamp_ms: u64,
    pub price_usd: f64,
    pub base_amount: f64,
}
#[derive(Default)]
pub struct TradeLedger {
    seen: HashSet<(String, u64, u16, Option<u16>)>,
    swaps: Vec<Swap>,
}
impl TradeLedger {
    pub fn insert(
        &mut self,
        s: VerifiedDexSwap,
        approved_dex_programs: &HashSet<String>,
    ) -> Result<bool, MarketError> {
        if !s.finalized || !approved_dex_programs.contains(&s.program) {
            return Err(MarketError::InvalidPrice);
        }
        if s.signature.len() < 40 || s.signature.len() > 100 {
            return Err(MarketError::InvalidVolume);
        }
        if !s.price_usd.is_finite()
            || s.price_usd <= 0.0
            || !s.base_amount.is_finite()
            || s.base_amount <= 0.0
        {
            return Err(MarketError::InvalidPrice);
        }
        let key = (s.signature, s.slot, s.ix, s.inner_ix);
        if self.seen.contains(&key) {
            return Ok(false);
        }
        if self
            .swaps
            .last()
            .is_some_and(|prev| prev.timestamp_ms > s.timestamp_ms)
        {
            return Err(MarketError::OutOfOrder);
        }
        self.seen.insert(key);
        self.swaps.push(Swap {
            timestamp_ms: s.timestamp_ms,
            price_usd: s.price_usd,
            base_amount: s.base_amount,
        });
        Ok(true)
    }
    pub fn swaps(&self) -> &[Swap] {
        &self.swaps
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn swap() -> VerifiedDexSwap {
        VerifiedDexSwap {
            signature: "12345678901234567890123456789012345678901234567890".to_string(),
            slot: 100,
            ix: 0,
            inner_ix: None,
            program: "allow-dex".to_string(),
            finalized: true,
            timestamp_ms: 30_000,
            price_usd: 1.5,
            base_amount: 4.0,
        }
    }
    #[test]
    fn deduplicates_instructions() {
        let mut l = TradeLedger::default();
        let allow = HashSet::from(["allow-dex".into()]);
        assert_eq!(l.insert(swap(), &allow), Ok(true));
        assert_eq!(l.insert(swap(), &allow), Ok(false));
        assert_eq!(l.swaps().len(), 1)
    }
    #[test]
    fn different_instruction_counts_as_new() {
        let mut l = TradeLedger::default();
        let allow = HashSet::from(["allow-dex".into()]);
        l.insert(swap(), &allow).unwrap();
        let mut other = swap();
        other.ix = 1;
        assert_eq!(l.insert(other, &allow), Ok(true));
        assert_eq!(l.swaps().len(), 2)
    }
    #[test]
    fn rejects_unapproved_program() {
        let mut l = TradeLedger::default();
        assert!(l.insert(swap(), &HashSet::new()).is_err())
    }
    #[test]
    fn requires_finalized() {
        let mut l = TradeLedger::default();
        let mut s = swap();
        s.finalized = false;
        assert!(l.insert(s, &HashSet::from(["allow-dex".into()])).is_err())
    }
}
