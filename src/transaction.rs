//! Bounded, read-only finalized transaction inspection.
//! A transaction is never classified as a DEX swap by transfer logs alone.
use crate::rpc::{RpcError, SolanaRpc};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalizedTransaction {
    pub slot: u64,
    pub block_time: Option<i64>,
    pub succeeded: bool,
    pub fee_lamports: u64,
    pub account_keys: Vec<String>,
}
pub fn parse_finalized_transaction(
    value: &Value,
) -> Result<Option<FinalizedTransaction>, RpcError> {
    let tx = value.get("result").ok_or(RpcError::InvalidResponse)?;
    if tx.is_null() {
        return Ok(None);
    }
    let slot = tx
        .get("slot")
        .and_then(Value::as_u64)
        .ok_or(RpcError::InvalidResponse)?;
    let meta = tx.get("meta").ok_or(RpcError::InvalidResponse)?;
    let err = meta.get("err").ok_or(RpcError::InvalidResponse)?;
    let fee_lamports = meta
        .get("fee")
        .and_then(Value::as_u64)
        .ok_or(RpcError::InvalidResponse)?;
    let keys = tx
        .pointer("/transaction/message/accountKeys")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    if keys.len() > 256 {
        return Err(RpcError::InvalidResponse);
    }
    let mut account_keys = Vec::with_capacity(keys.len());
    for key in keys {
        let address = key
            .as_str()
            .or_else(|| key.get("pubkey").and_then(Value::as_str))
            .ok_or(RpcError::InvalidResponse)?;
        let bytes = bs58::decode(address)
            .into_vec()
            .map_err(|_| RpcError::InvalidResponse)?;
        if bytes.len() != 32 {
            return Err(RpcError::InvalidResponse);
        }
        account_keys.push(address.to_owned());
    }
    let block_time = match tx.get("blockTime") {
        None | Some(Value::Null) => None,
        Some(t) => Some(t.as_i64().ok_or(RpcError::InvalidResponse)?),
    };
    Ok(Some(FinalizedTransaction {
        slot,
        block_time,
        succeeded: err.is_null(),
        fee_lamports,
        account_keys,
    }))
}
impl SolanaRpc {
    pub fn finalized_transaction(
        &self,
        signature: &str,
    ) -> Result<Option<FinalizedTransaction>, RpcError> {
        let bytes = bs58::decode(signature)
            .into_vec()
            .map_err(|_| RpcError::InvalidAddress)?;
        if bytes.len() != 64 {
            return Err(RpcError::InvalidAddress);
        }
        let response=self.call("getTransaction",
            json!([signature,{"encoding":"json","commitment":"finalized","maxSupportedTransactionVersion":0}]))?;
        parse_finalized_transaction(&response)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const KEY: &str = "So11111111111111111111111111111111111111112";
    #[test]
    fn parses_finalized_no_trade_inference() {
        let r = json!({"result":{"slot":88,"blockTime":null,"meta":{"err":null,"fee":5000},
         "transaction":{"message":{"accountKeys":[KEY]}}}});
        let parsed = parse_finalized_transaction(&r).unwrap().unwrap();
        assert_eq!(parsed.slot, 88);
        assert_eq!(parsed.fee_lamports, 5000);
        assert!(parsed.succeeded);
        assert_eq!(parsed.account_keys, vec![KEY.to_owned()]);
    }
    #[test]
    fn transaction_unavailable_is_explicit() {
        assert_eq!(
            parse_finalized_transaction(&json!({"result":null})),
            Ok(None)
        );
    }
    #[test]
    fn rejects_untrusted_account_keys_and_fee() {
        let r = json!({"result":{"slot":88,"meta":{"err":null,"fee":5000},
          "transaction":{"message":{"accountKeys":["bad"]}}}});
        assert_eq!(
            parse_finalized_transaction(&r),
            Err(RpcError::InvalidResponse)
        );
        let r = json!({"result":{"slot":88,"meta":{"err":null,"fee":-1},
          "transaction":{"message":{"accountKeys":[KEY]}}}});
        assert_eq!(
            parse_finalized_transaction(&r),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn invalid_signature_fails_before_transport() {
        let rpc = SolanaRpc::new("http://127.0.0.1:1".into()).unwrap();
        assert_eq!(
            rpc.finalized_transaction("bad"),
            Err(RpcError::InvalidAddress)
        );
    }
}
