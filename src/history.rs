//! Strict page validation for an archival signature scan.
use crate::rpc::{RecentSignature, RpcError};
use std::collections::HashSet;
#[derive(Default)]
pub struct HistoryScan {
    seen: HashSet<String>,
    oldest_slot: Option<u64>,
    pages: usize,
}
impl HistoryScan {
    pub fn ingest_page(
        &mut self,
        rows: &[RecentSignature],
        page_limit: usize,
    ) -> Result<Option<String>, RpcError> {
        if page_limit == 0 || page_limit > 1000 || rows.len() > page_limit {
            return Err(RpcError::InvalidResponse);
        }
        let mut local = HashSet::new();
        for (i, row) in rows.iter().enumerate() {
            if row.signature.is_empty()
                || !local.insert(row.signature.clone())
                || self.seen.contains(&row.signature)
                || i > 0 && rows[i - 1].slot < row.slot
                || self.oldest_slot.is_some_and(|previous| row.slot > previous)
            {
                return Err(RpcError::InvalidResponse);
            }
        }
        for row in rows {
            self.seen.insert(row.signature.clone());
        }
        if let Some(last) = rows.last() {
            self.oldest_slot = Some(last.slot);
        }
        self.pages += 1;
        Ok(if rows.len() == page_limit {
            rows.last().map(|x| x.signature.clone())
        } else {
            None
        })
    }
    pub fn unique_count(&self) -> usize {
        self.seen.len()
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(sig: &str, slot: u64) -> RecentSignature {
        RecentSignature {
            signature: sig.into(),
            slot,
            block_time: Some(1),
            has_error: false,
        }
    }
    #[test]
    fn history_pages_respect_cursor_and_order() {
        let mut s = HistoryScan::default();
        assert_eq!(
            s.ingest_page(&[row("a", 9), row("b", 8)], 2),
            Ok(Some("b".into()))
        );
        assert_eq!(s.ingest_page(&[row("c", 7)], 2), Ok(None));
        assert_eq!(s.unique_count(), 3);
        assert_eq!(s.pages(), 2);
    }
    #[test]
    fn duplicates_and_missing_history_fail_closed() {
        let mut s = HistoryScan::default();
        assert!(s.ingest_page(&[row("a", 5), row("a", 4)], 2).is_err());
        assert_eq!(s.unique_count(), 0);
        s.ingest_page(&[row("a", 5)], 1).unwrap();
        assert!(s.ingest_page(&[row("a", 5)], 1).is_err());
        assert!(s.ingest_page(&[row("b", 6)], 1).is_err());
        assert_eq!(s.unique_count(), 1);
    }
}

[executed on device: 6a59e52b5668 (5255b4bd-412b-4ac8-9b22-0b77cd4dee13)]