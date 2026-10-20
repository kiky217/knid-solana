//! Observation-time Solana log feed messages.
//! Received timestamps are local wall-clock observations, NOT block execution times.
use crate::rpc::RpcError;
use serde_json::Value;
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
