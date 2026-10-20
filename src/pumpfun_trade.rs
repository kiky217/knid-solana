//! Decode on-chain Pump.fun Anchor TradeEvent logs (legacy emit! format).
//! Only logs emitted while the Pump.fun program is the active stack frame qualify.
//! Prices are denominated in SOL, not USD.
use crate::pumpfun_candidate::PUMPFUN_PROGRAM;
use crate::rpc::RpcError;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

const TRADE_DISC: [u8; 8] = [0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PumpTradeEvent {
    pub mint: String,
    pub sol_lamports: u64,
    pub token_raw: u64,
    pub is_buy: bool,
    pub timestamp_s: i64,
    pub event_index: usize,
}
fn read_u64(bytes: &[u8], start: usize) -> Result<u64, RpcError> {
    let v = bytes
        .get(start..start + 8)
        .ok_or(RpcError::InvalidResponse)?;
    Ok(u64::from_le_bytes(
        v.try_into().map_err(|_| RpcError::InvalidResponse)?,
    ))
}
fn decode_payload(data: &[u8], idx: usize) -> Result<Option<PumpTradeEvent>, RpcError> {
    if data.len() < 8 || data[..8] != TRADE_DISC {
        return Ok(None);
    }
    if data.len() < 8 + 121 {
        return Err(RpcError::InvalidResponse);
    }
    let body = &data[8..];
    let mint = bs58::encode(&body[0..32]).into_string();
    let sol_lamports = read_u64(body, 32)?;
    let token_raw = read_u64(body, 40)?;
    let is_buy = match body[48] {
        0 => false,
        1 => true,
        _ => return Err(RpcError::InvalidResponse),
    };
    let timestamp_s = i64::from_le_bytes(
        body[81..89]
            .try_into()
            .map_err(|_| RpcError::InvalidResponse)?,
    );
    if sol_lamports == 0 || token_raw == 0 || timestamp_s <= 0 {
        return Err(RpcError::InvalidResponse);
    }
    Ok(Some(PumpTradeEvent {
        mint,
        sol_lamports,
        token_raw,
        is_buy,
        timestamp_s,
        event_index: idx,
    }))
}
fn invoke_program(line: &str) -> Option<&str> {
    let tail = line.strip_prefix("Program ")?;
    let (id, _) = tail.split_once(" invoke [")?;
    Some(id)
}
fn completion_program(line: &str) -> Option<&str> {
    let tail = line.strip_prefix("Program ")?;
    tail.strip_suffix(" success")
        .or_else(|| tail.split_once(" failed:").map(|x| x.0))
}
pub fn decode_pumpfun_trade_logs(response: &Value) -> Result<Vec<PumpTradeEvent>, RpcError> {
    let tx = response
        .get("result")
        .filter(|v| !v.is_null())
        .ok_or(RpcError::InvalidResponse)?;
    let meta = tx.get("meta").ok_or(RpcError::InvalidResponse)?;
    if !meta.get("err").is_some_and(Value::is_null) {
        return Ok(vec![]);
    }
    let logs = meta
        .get("logMessages")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    // Partial logs cannot establish a complete set of swaps for this transaction.
    if logs
        .iter()
        .any(|line| line.as_str() == Some("Log truncated"))
    {
        return Err(RpcError::InvalidResponse);
    }
    let mut stack: Vec<&str> = Vec::new();
    let mut events = Vec::new();
    for line in logs {
        let line = line.as_str().ok_or(RpcError::InvalidResponse)?;
        if let Some(program) = invoke_program(line) {
            stack.push(program);
            continue;
        }
        if let Some(program) = completion_program(line) {
            if stack.pop() != Some(program) {
                return Err(RpcError::InvalidResponse);
            }
            continue;
        }
        if let Some(encoded) = line.strip_prefix("Program data: ") {
            if stack.last().copied() != Some(PUMPFUN_PROGRAM) {
                continue;
            }
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|_| RpcError::InvalidResponse)?;
            if let Some(event) = decode_payload(&bytes, events.len())? {
                events.push(event)
            }
        }
    }
    if !stack.is_empty() {
        return Err(RpcError::InvalidResponse);
    }
    Ok(events)
}
impl PumpTradeEvent {
    /// Executed SOL per token. Caller must pass token mint decimals proven via RPC.
    pub fn price_sol(&self, decimals: u8) -> Result<f64, RpcError> {
        if decimals > 18 {
            return Err(RpcError::InvalidSupply);
        }
        let base = (self.token_raw as f64) / 10_f64.powi(decimals as i32);
        if base <= 0.0 {
            return Err(RpcError::InvalidSupply);
        }
        let price = (self.sol_lamports as f64) / 1_000_000_000.0 / base;
        if price.is_finite() && price > 0.0 {
            Ok(price)
        } else {
            Err(RpcError::InvalidSupply)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn log_data() -> String {
        let mut d = TRADE_DISC.to_vec();
        d.extend_from_slice(&[1_u8; 32]);
        d.extend_from_slice(&1_000_000_000_u64.to_le_bytes());
        d.extend_from_slice(&2_000_000_u64.to_le_bytes());
        d.push(1);
        d.extend_from_slice(&[0_u8; 32]);
        d.extend_from_slice(&1_700_000_000_i64.to_le_bytes());
        d.extend_from_slice(&[0_u8; 32]);
        format!("Program data: {}", STANDARD.encode(d))
    }
    fn response(logs: Vec<String>) -> Value {
        serde_json::json!({"result":{"meta":{"err":null,"logMessages":logs}}})
    }
    #[test]
    fn decodes_real_event_layout_and_price() {
        let e = decode_pumpfun_trade_logs(&response(vec![
            format!("Program {PUMPFUN_PROGRAM} invoke [1]"),
            log_data(),
            format!("Program {PUMPFUN_PROGRAM} success"),
        ]))
        .unwrap();
        assert_eq!(e.len(), 1);
        assert!(e[0].is_buy);
        assert_eq!(e[0].price_sol(6), Ok(0.5));
    }
    #[test]
    fn rejects_same_payload_from_untrusted_program() {
        assert!(decode_pumpfun_trade_logs(&response(vec![
            "Program Fake invoke [1]".into(),
            log_data(),
            "Program Fake success".into()
        ]))
        .unwrap()
        .is_empty());
    }
    #[test]
    fn rejects_truncated_trade_payload() {
        let log = format!("Program data: {}", STANDARD.encode(TRADE_DISC));
        assert_eq!(
            decode_pumpfun_trade_logs(&response(vec![
                format!("Program {PUMPFUN_PROGRAM} invoke [1]"),
                log,
                format!("Program {PUMPFUN_PROGRAM} success")
            ])),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn rejects_truncated_logs_even_after_a_valid_event() {
        assert_eq!(
            decode_pumpfun_trade_logs(&response(vec![
                format!("Program {PUMPFUN_PROGRAM} invoke [1]"),
                log_data(),
                format!("Program {PUMPFUN_PROGRAM} success"),
                "Log truncated".into(),
            ])),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn no_trade_from_failed_transaction() {
        let mut r = response(vec![]);
        r["result"]["meta"]["err"] = serde_json::json!({"Custom":1});
        assert_eq!(decode_pumpfun_trade_logs(&r), Ok(vec![]));
    }
}
