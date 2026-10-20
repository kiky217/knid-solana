//! Bounded finalized account history scan. Not complete all-pool archival coverage.
use crate::history::HistoryScan;
use crate::pumpfun_trade::{decode_pumpfun_trade_logs, PumpTradeEvent};
use crate::rpc::{RpcError, SolanaRpc};
use serde_json::json;
use std::time::Duration;

// Retry 8s, then 60s, then return to 8s. Other errors fail closed.
fn retry_rate_limit<T, F: FnMut() -> Result<T, RpcError>>(mut action: F) -> Result<T, RpcError> {
    for seconds in [0, 8, 60, 8] {
        if seconds > 0 {
            std::thread::sleep(Duration::from_secs(seconds));
        }
        match action() {
            Err(RpcError::RateLimited) => continue,
            result => return result,
        }
    }
    Err(RpcError::RateLimited)
}
#[derive(Debug, Clone)]
pub struct HistoricalTrade {
    pub signature: String,
    pub slot: u64,
    pub event: PumpTradeEvent,
    pub price_sol: f64,
}
#[derive(Debug)]
pub struct HistoryResult {
    pub trades: Vec<HistoricalTrade>,
    pub scanned_signatures: usize,
    pub pages: usize,
    pub next_cursor: Option<String>,
    pub reached_oldest: bool,
    pub coverage_scope: &'static str,
}
/// Scan a bounded mint-address history; pools require separate scans.
pub fn scan_mint_trade_events(
    rpc: &SolanaRpc,
    mint: &str,
    page_limit: usize,
    max_pages: usize,
) -> Result<HistoryResult, RpcError> {
    scan_mint_trade_events_from(rpc, mint, page_limit, max_pages, None)
}

pub fn scan_mint_trade_events_from(
    rpc: &SolanaRpc,
    mint: &str,
    page_limit: usize,
    max_pages: usize,
    start_cursor: Option<&str>,
) -> Result<HistoryResult, RpcError> {
    scan_events_for_address_from(
        rpc,
        mint,
        mint,
        page_limit,
        max_pages,
        start_cursor,
        "mint_address_only_not_all_pools",
    )
}

/// Scan pool-address history, verifying Pump.fun account ownership and filtering
/// decoded trade events by target mint. This is not proof of exhaustive DEX coverage.
pub fn scan_pool_trade_events_from(
    rpc: &SolanaRpc,
    mint: &str,
    pool_address: &str,
    page_limit: usize,
    max_pages: usize,
    start_cursor: Option<&str>,
) -> Result<HistoryResult, RpcError> {
    crate::rpc::validate_mint(pool_address)?;
    let state = rpc.call(
        "getAccountInfo",
        json!([pool_address, {"encoding":"base64","commitment":"finalized"}]),
    )?;
    let owner = state
        .pointer("/result/value/owner")
        .and_then(serde_json::Value::as_str)
        .ok_or(RpcError::InvalidResponse)?;
    if owner != crate::pumpfun_candidate::PUMPFUN_PROGRAM {
        return Err(RpcError::ProgramMismatch);
    }
    scan_events_for_address_from(
        rpc,
        mint,
        pool_address,
        page_limit,
        max_pages,
        start_cursor,
        "pumpfun_pool_address_only_not_all_venues",
    )
}

fn scan_events_for_address_from(
    rpc: &SolanaRpc,
    mint: &str,
    history_address: &str,
    page_limit: usize,
    max_pages: usize,
    start_cursor: Option<&str>,
    coverage_scope: &'static str,
) -> Result<HistoryResult, RpcError> {
    if let Some(signature) = start_cursor {
        if !bs58::decode(signature)
            .into_vec()
            .is_ok_and(|v| v.len() == 64)
        {
            return Err(RpcError::InvalidAddress);
        }
    }
    if page_limit == 0 || page_limit > 100 || max_pages == 0 || max_pages > 10 {
        return Err(RpcError::InvalidResponse);
    }
    let (_, decimals, _) = retry_rate_limit(|| rpc.token_snapshot(mint))?;
    let mut cursor: Option<String> = start_cursor.map(str::to_owned);
    let mut scan = HistoryScan::default();
    let mut trades = Vec::new();
    let mut reached_oldest = false;
    for _ in 0..max_pages {
        let rows = retry_rate_limit(|| {
            rpc.signatures_page(history_address, page_limit, cursor.as_deref())
        })?;
        let next = scan.ingest_page(&rows, page_limit)?;
        for row in &rows {
            if row.has_error {
                continue;
            }
            std::thread::sleep(Duration::from_millis(750));
            let response = retry_rate_limit(|| {
                rpc.call("getTransaction",json!([
                row.signature,{"encoding":"jsonParsed","commitment":"finalized","maxSupportedTransactionVersion":1}
            ]))
            })?;
            let tx = response
                .get("result")
                .filter(|x| !x.is_null())
                .ok_or(RpcError::InvalidResponse)?;
            if tx.get("slot").and_then(serde_json::Value::as_u64) != Some(row.slot) {
                return Err(RpcError::InvalidResponse);
            }
            let events = decode_pumpfun_trade_logs(&response)?;
            for event in events {
                if event.mint == mint {
                    let price_sol = event.price_sol(decimals)?;
                    trades.push(HistoricalTrade {
                        signature: row.signature.clone(),
                        slot: row.slot,
                        event,
                        price_sol,
                    });
                }
            }
        }
        cursor = next;
        if cursor.is_none() {
            reached_oldest = true;
            break;
        }
    }
    Ok(HistoryResult {
        trades,
        scanned_signatures: scan.unique_count(),
        pages: scan.pages(),
        next_cursor: cursor,
        reached_oldest,
        coverage_scope,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_backfill_bounds() {
        let rpc = SolanaRpc::new("http://127.0.0.1:1".into()).unwrap();
        assert!(matches!(
            scan_mint_trade_events(&rpc, "invalid", 0, 1),
            Err(RpcError::InvalidResponse)
        ));
        assert!(matches!(
            scan_mint_trade_events(&rpc, "invalid", 1, 11),
            Err(RpcError::InvalidResponse)
        ));
        assert!(matches!(
            scan_mint_trade_events_from(&rpc, "invalid", 1, 1, Some("bad")),
            Err(RpcError::InvalidAddress)
        ));
    }
}
