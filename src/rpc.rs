//! Read-only Solana RPC; all network results are untrusted and explicitly validated.
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;
#[derive(Debug, PartialEq)]
pub enum RpcError {
    InvalidAddress,
    InvalidEndpoint,
    Transport,
    InvalidResponse,
    ProgramMismatch,
    InvalidSupply,
}
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub fn validate_mint(mint: &str) -> Result<(), RpcError> {
    let b = bs58::decode(mint)
        .into_vec()
        .map_err(|_| RpcError::InvalidAddress)?;
    if b.len() != 32 {
        return Err(RpcError::InvalidAddress);
    }
    Ok(())
}
pub fn parse_supply(value: &Value) -> Result<(u64, u8), RpcError> {
    let v = value
        .get("result")
        .and_then(|x| x.get("value"))
        .ok_or(RpcError::InvalidResponse)?;
    let s = v
        .get("amount")
        .and_then(Value::as_str)
        .ok_or(RpcError::InvalidSupply)?;
    let n = s.parse::<u64>().map_err(|_| RpcError::InvalidSupply)?;
    let decimals = v
        .get("decimals")
        .and_then(Value::as_u64)
        .ok_or(RpcError::InvalidSupply)?;
    if decimals > 18 {
        return Err(RpcError::InvalidSupply);
    }
    Ok((n, decimals as u8))
}
pub fn parse_mint_owner(value: &Value) -> Result<String, RpcError> {
    let v = value
        .get("result")
        .and_then(|x| x.get("value"))
        .ok_or(RpcError::InvalidResponse)?;
    let owner = v
        .get("owner")
        .and_then(Value::as_str)
        .ok_or(RpcError::InvalidResponse)?;
    if owner != TOKEN_PROGRAM && owner != TOKEN_2022_PROGRAM {
        return Err(RpcError::ProgramMismatch);
    }
    if v.get("executable").and_then(Value::as_bool) != Some(false) {
        return Err(RpcError::InvalidResponse);
    }
    Ok(owner.to_owned())
}
pub struct SolanaRpc {
    endpoint: String,
}
impl SolanaRpc {
    pub fn new(endpoint: String) -> Result<Self, RpcError> {
        if !(endpoint.starts_with("https://")
            || endpoint.starts_with("http://127.0.0.1:")
            || endpoint.starts_with("http://localhost:"))
        {
            return Err(RpcError::InvalidEndpoint);
        }
        if endpoint.contains('@') || endpoint.contains('#') || endpoint.contains(' ') {
            return Err(RpcError::InvalidEndpoint);
        }
        Ok(Self { endpoint })
    }
    pub fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if !matches!(
            method,
            "getAccountInfo"
                | "getTokenSupply"
                | "getSignaturesForAddress"
                | "getTransaction"
                | "getSlot"
        ) {
            return Err(RpcError::InvalidResponse);
        }
        let request = json!({"jsonrpc":"2.0","id":1,"method":method,"params":params});
        let response = ureq::post(&self.endpoint)
            .timeout(Duration::from_secs(10))
            .send_json(request)
            .map_err(|_| RpcError::Transport)?;
        const MAX_RPC_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
        let mut limited = response.into_reader().take(MAX_RPC_RESPONSE_BYTES + 1);
        let mut bytes = Vec::new();
        limited
            .read_to_end(&mut bytes)
            .map_err(|_| RpcError::Transport)?;
        if bytes.len() as u64 > MAX_RPC_RESPONSE_BYTES {
            return Err(RpcError::InvalidResponse);
        }
        let result: Value =
            serde_json::from_slice(&bytes).map_err(|_| RpcError::InvalidResponse)?;
        if result.get("id") != Some(&json!(1))
            || result.get("jsonrpc") != Some(&json!("2.0"))
            || result.get("error").is_some()
        {
            return Err(RpcError::InvalidResponse);
        }
        if result.get("result").is_none() {
            return Err(RpcError::InvalidResponse);
        }
        Ok(result)
    }
    pub fn token_snapshot(&self, mint: &str) -> Result<(u64, u8, String), RpcError> {
        validate_mint(mint)?;
        let info = self.call(
            "getAccountInfo",
            json!([mint,{"encoding":"base64","commitment":"finalized"}]),
        )?;
        let owner = parse_mint_owner(&info)?;
        let supply = self.call("getTokenSupply", json!([mint,{"commitment":"finalized"}]))?;
        let (amount, decimals) = parse_supply(&supply)?;
        Ok((amount, decimals, owner))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_base58_public_key() {
        assert!(validate_mint("So11111111111111111111111111111111111111112").is_ok());
        assert_eq!(validate_mint("invalid"), Err(RpcError::InvalidAddress))
    }
    #[test]
    fn reads_supply_integer_only() {
        assert_eq!(
            parse_supply(&json!({"result":{"value":{"amount":"100000","decimals":6}}})),
            Ok((100000, 6))
        );
        assert!(parse_supply(&json!({"result":{"value":{"amount":"NaN","decimals":6}}})).is_err())
    }
    #[test]
    fn rejects_other_account_owners() {
        assert_eq!(
            parse_mint_owner(
                &json!({"result":{"value":{"owner":"11111111111111111111111111111111","executable":false}}})
            ),
            Err(RpcError::ProgramMismatch)
        )
    }
    #[test]
    fn accepts_real_token_2022_owner() {
        assert_eq!(
            TOKEN_2022_PROGRAM,
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        );
        let r = json!({"result":{"value":{"owner":TOKEN_2022_PROGRAM,"executable":false}}});
        assert_eq!(parse_mint_owner(&r), Ok(TOKEN_2022_PROGRAM.to_owned()));
    }
    #[test]
    fn rejects_credentials_in_url() {
        assert!(SolanaRpc::new("https://user:pass@rpc.example/".into()).is_err())
    }
    #[test]
    fn rejects_plain_remote_http() {
        assert!(SolanaRpc::new("http://public.example.com".into()).is_err())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentSignature {
    pub signature: String,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub has_error: bool,
}
pub fn parse_signatures(value: &Value, maximum: usize) -> Result<Vec<RecentSignature>, RpcError> {
    let list = value
        .get("result")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    if list.len() > maximum {
        return Err(RpcError::InvalidResponse);
    }
    list.iter()
        .map(|item| {
            let signature = item
                .get("signature")
                .and_then(Value::as_str)
                .ok_or(RpcError::InvalidResponse)?;
            if signature.len() < 40
                || signature.len() > 100
                || bs58::decode(signature).into_vec().is_err()
            {
                return Err(RpcError::InvalidResponse);
            }
            let slot = item
                .get("slot")
                .and_then(Value::as_u64)
                .ok_or(RpcError::InvalidResponse)?;
            let block_time = match item.get("blockTime") {
                Some(Value::Null) | None => None,
                Some(v) => Some(v.as_i64().ok_or(RpcError::InvalidResponse)?),
            };
            Ok(RecentSignature {
                signature: signature.to_owned(),
                slot,
                block_time,
                has_error: !item.get("err").is_some_and(Value::is_null),
            })
        })
        .collect()
}
impl SolanaRpc {
    pub fn recent_signatures(
        &self,
        address: &str,
        limit: usize,
    ) -> Result<Vec<RecentSignature>, RpcError> {
        self.signatures_page(address, limit, None)
    }
    /// Read-only historical pagination. `before` is an exclusive oldest-signature cursor.
    pub fn signatures_page(
        &self,
        address: &str,
        limit: usize,
        before: Option<&str>,
    ) -> Result<Vec<RecentSignature>, RpcError> {
        validate_mint(address)?;
        if let Some(cursor) = before {
            let bytes = bs58::decode(cursor)
                .into_vec()
                .map_err(|_| RpcError::InvalidAddress)?;
            if bytes.len() != 64 {
                return Err(RpcError::InvalidAddress);
            }
        }
        if !(1..=1000).contains(&limit) {
            return Err(RpcError::InvalidResponse);
        }
        let mut options = json!({"limit":limit,"commitment":"finalized"});
        if let Some(cursor) = before {
            options["before"] = json!(cursor);
        }
        let result = self.call("getSignaturesForAddress", json!([address, options]))?;
        parse_signatures(&result, limit)
    }
}
#[cfg(test)]
mod more_rpc_tests {
    use super::*;
    #[test]
    fn parses_finalized_signatures() {
        let signature = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz123456789";
        let result =
            json!({"result":[{"signature":signature,"slot":123,"blockTime":1234567,"err":null}]});
        let parsed = parse_signatures(&result, 10).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].slot, 123);
        assert!(!parsed[0].has_error)
    }
    #[test]
    fn rejects_untrusted_signature() {
        assert_eq!(
            parse_signatures(
                &json!({"result":[{"signature":"bad","slot":1,"err":null}]}),
                10
            ),
            Err(RpcError::InvalidResponse)
        )
    }
    #[test]
    fn history_cursor_must_be_valid_signature() {
        let rpc = SolanaRpc::new("http://127.0.0.1:1".into()).unwrap();
        assert_eq!(
            rpc.signatures_page(
                "So11111111111111111111111111111111111111112",
                100,
                Some("bad")
            ),
            Err(RpcError::InvalidAddress)
        );
        assert_eq!(
            rpc.signatures_page("So11111111111111111111111111111111111111112", 1001, None),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn rejects_oversized_page() {
        assert_eq!(
            parse_signatures(&json!({"result":[{},{}]}), 1),
            Err(RpcError::InvalidResponse)
        )
    }
}

[executed on device: 6a59e52b5668 (5255b4bd-412b-4ac8-9b22-0b77cd4dee13)]