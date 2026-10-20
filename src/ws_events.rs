//! Observation-time Solana log feed messages.
//! Received timestamps are local wall-clock observations, NOT block execution times.
use crate::pumpfun_trade::{decode_pumpfun_trade_logs, PumpTradeEvent};
use crate::rpc::RpcError;
use serde_json::{json, Value};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedSignature {
    pub signature: String,
    pub slot: u64,
    pub received_at_ns: u128,
    pub execution_succeeded: bool,
    pub commitment: &'static str,
}
pub fn parse_logs_notification(
    value: &Value,
    received_at_ns: u128,
) -> Result<Option<ObservedSignature>, RpcError> {
    if value.get("method").and_then(Value::as_str) != Some("logsNotification") {
        return Ok(None);
    }
    if received_at_ns == 0 {
        return Err(RpcError::InvalidResponse);
    }
    let result = value
        .pointer("/params/result")
        .ok_or(RpcError::InvalidResponse)?;
    let slot = result
        .pointer("/context/slot")
        .and_then(Value::as_u64)
        .filter(|x| *x > 0)
        .ok_or(RpcError::InvalidResponse)?;
    let tx = result.get("value").ok_or(RpcError::InvalidResponse)?;
    let sig = tx
        .get("signature")
        .and_then(Value::as_str)
        .ok_or(RpcError::InvalidResponse)?;
    let bytes = bs58::decode(sig)
        .into_vec()
        .map_err(|_| RpcError::InvalidResponse)?;
    if bytes.len() != 64 {
        return Err(RpcError::InvalidResponse);
    }
    let err = tx.get("err").ok_or(RpcError::InvalidResponse)?;
    let logs = tx
        .get("logs")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    if logs.len() > 8192 || logs.iter().any(|l| l.as_str().is_none()) {
        return Err(RpcError::InvalidResponse);
    }
    Ok(Some(ObservedSignature {
        signature: sig.to_owned(),
        slot,
        received_at_ns,
        execution_succeeded: err.is_null(),
        commitment: "processed",
    }))
}
/// Verified Pump.fun log payload observed at processed commitment.
/// Provisional until independently matched to a finalized transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvisionalPumpTrade {
    pub signature: String,
    pub slot: u64,
    pub received_at_ns: u128,
    pub event: PumpTradeEvent,
}
pub fn parse_provisional_pump_trades(
    value: &Value,
    received_at_ns: u128,
) -> Result<Vec<ProvisionalPumpTrade>, RpcError> {
    let Some(observed) = parse_logs_notification(value, received_at_ns)? else {
        return Ok(vec![]);
    };
    if !observed.execution_succeeded {
        return Ok(vec![]);
    }
    let logs = value
        .pointer("/params/result/value/logs")
        .ok_or(RpcError::InvalidResponse)?;
    let wrapped = json!({"result":{"meta":{"err":null,"logMessages":logs}}});
    let events = decode_pumpfun_trade_logs(&wrapped)?;
    Ok(events
        .into_iter()
        .map(|event| ProvisionalPumpTrade {
            signature: observed.signature.clone(),
            slot: observed.slot,
            received_at_ns,
            event,
        })
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    const SIG: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    #[test]
    fn observed_tick_has_local_ns_not_chain_ms() {
        let v = serde_json::json!({"method":"logsNotification","params":{"result":{
            "context":{"slot":123},"value":{"signature":SIG,"err":null,"logs":[]}}}});
        let tick = parse_logs_notification(&v, 1_797_000_123_456_789_000)
            .unwrap()
            .unwrap();
        assert_eq!(tick.slot, 123);
        assert_eq!(tick.received_at_ns, 1_797_000_123_456_789_000);
        assert_eq!(tick.commitment, "processed");
        assert!(tick.execution_succeeded);
    }
    #[test]
    fn provisional_trade_requires_valid_program_stack_and_complete_logs() {
        use crate::pumpfun_candidate::PUMPFUN_PROGRAM;
        use base64::{engine::general_purpose::STANDARD, Engine};
        let mut bytes = vec![0xbd, 0xdb, 0x7f, 0xd3, 0x4e, 0xe6, 0x61, 0xee];
        bytes.extend_from_slice(&[1; 32]);
        bytes.extend_from_slice(&1_000_000_000_u64.to_le_bytes());
        bytes.extend_from_slice(&2_000_000_u64.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&[0; 32]);
        bytes.extend_from_slice(&1_700_000_000_i64.to_le_bytes());
        bytes.extend_from_slice(&[0; 32]);
        let logs = vec![
            format!("Program {PUMPFUN_PROGRAM} invoke [1]"),
            format!("Program data: {}", STANDARD.encode(bytes)),
            format!("Program {PUMPFUN_PROGRAM} success"),
        ];
        let mut notification = json!({"method":"logsNotification","params":{"result":{
            "context":{"slot":123},"value":{"signature":SIG,"err":null,"logs":logs}
        }}});
        let observed =
            parse_provisional_pump_trades(&notification, 1_797_000_000_123_456_789).unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(observed[0].slot, 123);
        assert_eq!(observed[0].event.price_sol(6), Ok(0.5));
        notification["params"]["result"]["value"]["logs"]
            .as_array_mut()
            .unwrap()
            .push(json!("Log truncated"));
        assert_eq!(
            parse_provisional_pump_trades(&notification, 100),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn ignores_subscription_ack() {
        let v = serde_json::json!({"jsonrpc":"2.0","result":123,"id":1});
        assert_eq!(parse_logs_notification(&v, 100), Ok(None));
    }
    #[test]
    fn rejects_invalid_signature() {
        let v = serde_json::json!({"method":"logsNotification","params":{"result":{
            "context":{"slot":123},"value":{"signature":"bad","err":null,"logs":[]}}}});
        assert_eq!(
            parse_logs_notification(&v, 100),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn failed_transaction_is_observed_but_not_success() {
        let v = serde_json::json!({"method":"logsNotification","params":{"result":{
            "context":{"slot":123},"value":{"signature":SIG,"err":{"Custom":1},"logs":[]}}}});
        assert!(
            !parse_logs_notification(&v, 100)
                .unwrap()
                .unwrap()
                .execution_succeeded
        );
    }
}
