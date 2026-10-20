//! Combine overlapping CA/pool history safely; completion remains source-specific.
use crate::backfill::{HistoricalTrade, HistoryResult};
use crate::rpc::RpcError;
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct MergedHistory {
    pub trades: Vec<HistoricalTrade>,
    pub duplicate_events: usize,
    pub source_count: usize,
    pub unresolved_signatures: Vec<String>,
    pub all_sources_reached_oldest: bool,
    pub complete_all_venues: bool,
}
/// A deduplicated union of *observed* trade events, NOT guaranteed global coverage.
pub fn merge_history(results: Vec<HistoryResult>) -> Result<MergedHistory, RpcError> {
    if results.is_empty() {
        return Err(RpcError::InvalidResponse);
    }
    let mut unresolved_signatures: Vec<String> = results
        .iter()
        .flat_map(|r| r.unresolved_signatures.iter().cloned())
        .collect();
    unresolved_signatures.sort();
    unresolved_signatures.dedup();
    let all_sources_reached_oldest = unresolved_signatures.is_empty()
        && results
            .iter()
            .all(|r| r.reached_oldest && r.next_cursor.is_none());
    let source_count = results.len();
    let mut events: BTreeMap<(String, usize), HistoricalTrade> = BTreeMap::new();
    let mut duplicate_events = 0;
    for batch in results {
        for trade in batch.trades {
            let key = (trade.signature.clone(), trade.event.event_index);
            if let Some(old) = events.get(&key) {
                if old.slot != trade.slot
                    || old.event != trade.event
                    || old.price_sol.to_bits() != trade.price_sol.to_bits()
                {
                    return Err(RpcError::InvalidResponse);
                }
                duplicate_events += 1;
            } else {
                events.insert(key, trade);
            }
        }
    }
    let mut trades: Vec<HistoricalTrade> = events.into_values().collect();
    trades.sort_by(|a, b| {
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
    Ok(MergedHistory {
        trades,
        duplicate_events,
        source_count,
        unresolved_signatures,
        all_sources_reached_oldest,
        complete_all_venues: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pumpfun_trade::PumpTradeEvent;
    fn tick(signature: &str, slot: u64, amount: u64) -> HistoricalTrade {
        HistoricalTrade {
            signature: signature.to_string(),
            slot,
            price_sol: 1.0,
            event: PumpTradeEvent {
                mint: "mint".into(),
                sol_lamports: amount,
                token_raw: 10,
                is_buy: true,
                timestamp_s: 1700000000,
                event_index: 0,
            },
        }
    }
    fn result(trades: Vec<HistoricalTrade>) -> HistoryResult {
        HistoryResult {
            trades,
            scanned_signatures: 10,
            pages: 1,
            next_cursor: None,
            reached_oldest: true,
            coverage_scope: "test_account_only",
            unresolved_signatures: vec![],
        }
    }
    #[test]
    fn deduplicates_ca_and_pool_overlap() {
        let m = merge_history(vec![
            result(vec![tick("a", 1, 10), tick("b", 2, 20)]),
            result(vec![tick("a", 1, 10), tick("c", 3, 30)]),
        ])
        .unwrap();
        assert_eq!(m.trades.len(), 3);
        assert_eq!(m.duplicate_events, 1);
        assert!(m.all_sources_reached_oldest);
        assert!(!m.complete_all_venues);
    }
    #[test]
    fn unresolved_gap_prevents_complete_coverage() {
        let mut r = result(vec![tick("a", 1, 10)]);
        r.unresolved_signatures.push("truncated-tx".into());
        let merged = merge_history(vec![r]).unwrap();
        assert_eq!(merged.unresolved_signatures, vec!["truncated-tx"]);
        assert!(!merged.all_sources_reached_oldest);
        assert!(!merged.complete_all_venues);
    }
    #[test]
    fn conflicting_duplicate_rejected() {
        assert!(merge_history(vec![
            result(vec![tick("a", 1, 10)]),
            result(vec![tick("a", 1, 999)])
        ])
        .is_err());
    }
    #[test]
    fn partial_source_does_not_claim_completion() {
        let mut r = result(vec![]);
        r.reached_oldest = false;
        r.next_cursor = Some("cursor".into());
        let m = merge_history(vec![r]).unwrap();
        assert!(!m.all_sources_reached_oldest);
        assert!(!m.complete_all_venues);
    }
}
