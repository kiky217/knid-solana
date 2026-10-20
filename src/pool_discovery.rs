//! Read-only, independently owner-checked Pump.fun pool discovery.
//! Indexer discovery is only a hint: actual mint/pool relationship still needs state decoding.
use crate::pumpfun_candidate::PUMPFUN_PROGRAM;
use crate::rpc::{validate_mint, RpcError, SolanaRpc};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Read;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PumpPoolHint {
    pub address: String,
    pub owner_program: String,
    pub onchain_owner_verified: bool,
    pub mint_membership_verified: bool,
}

pub fn parse_pump_pool_addresses(value: &Value) -> Result<Vec<String>, RpcError> {
    let pools = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    if pools.len() > 100 {
        return Err(RpcError::InvalidResponse);
    }
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for p in pools {
        if p.pointer("/relationships/dex/data/id")
            .and_then(Value::as_str)
            != Some("pump-fun")
        {
            continue;
        }
        let id = p
            .get("id")
            .and_then(Value::as_str)
            .ok_or(RpcError::InvalidResponse)?;
        let address = p
            .pointer("/attributes/address")
            .and_then(Value::as_str)
            .ok_or(RpcError::InvalidResponse)?;
        if id.strip_prefix("solana_") != Some(address) {
            return Err(RpcError::InvalidResponse);
        }
        validate_mint(address)?;
        if seen.insert(address.to_owned()) {
            found.push(address.to_owned())
        }
    }
    Ok(found)
}
pub fn discover_pumpfun_pools_from(
    rpc: &SolanaRpc,
    mint: &str,
    api_base: &str,
) -> Result<Vec<PumpPoolHint>, RpcError> {
    validate_mint(mint)?;
    if !(api_base.starts_with("https://")
        || api_base.starts_with("http://127.0.0.1:")
        || api_base.starts_with("http://localhost:"))
    {
        return Err(RpcError::InvalidEndpoint);
    }
    let uri = format!(
        "{}/networks/solana/tokens/{mint}/pools?page=1",
        api_base.trim_end_matches('/')
    );
    let response = ureq::get(&uri)
        .set("User-Agent", "KNID-Solana-readonly/1.0")
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(429, _) => RpcError::RateLimited,
            ureq::Error::Status(code, _) => RpcError::HttpStatus(code),
            _ => RpcError::Transport,
        })?;
    const LIMIT: u64 = 1_048_576;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RpcError::Transport)?;
    if bytes.len() as u64 > LIMIT {
        return Err(RpcError::InvalidResponse);
    }
    let data: Value = serde_json::from_slice(&bytes).map_err(|_| RpcError::InvalidResponse)?;
    let addresses = parse_pump_pool_addresses(&data)?;
    let mut result = Vec::new();
    for address in addresses {
        let acc = rpc.call(
            "getAccountInfo",
            json!([address,{"encoding":"base64","commitment":"finalized"}]),
        )?;
        let state = acc
            .pointer("/result/value")
            .filter(|x| !x.is_null())
            .ok_or(RpcError::InvalidResponse)?;
        let owner = state
            .get("owner")
            .and_then(Value::as_str)
            .ok_or(RpcError::InvalidResponse)?;
        if owner != PUMPFUN_PROGRAM {
            return Err(RpcError::ProgramMismatch);
        }
        if state.get("executable").and_then(Value::as_bool) != Some(false) {
            return Err(RpcError::InvalidResponse);
        }
        result.push(PumpPoolHint {
            address,
            owner_program: owner.to_owned(),
            onchain_owner_verified: true,
            mint_membership_verified: false,
        });
    }
    Ok(result)
}
pub fn discover_pumpfun_pools(rpc: &SolanaRpc, mint: &str) -> Result<Vec<PumpPoolHint>, RpcError> {
    discover_pumpfun_pools_from(rpc, mint, "https://api.geckoterminal.com/api/v2")
}
#[cfg(test)]
mod tests {
    use super::*;
    const ADDR: &str = "Gb6jnAQed7oQenNmQKG6kL7u6JstzzTajshJC2HEwmmt";
    #[test]
    fn accepts_indexer_hint_not_claiming_membership() {
        let data = json!({"data":[{"id":format!("solana_{ADDR}"),
            "attributes":{"address":ADDR},
            "relationships":{"dex":{"data":{"id":"pump-fun"}}}}]});
        assert_eq!(parse_pump_pool_addresses(&data), Ok(vec![ADDR.to_owned()]));
    }
    #[test]
    fn wrong_pool_identity_fails_closed() {
        let data = json!({"data":[{"id":"solana_other","attributes":{"address":ADDR},
            "relationships":{"dex":{"data":{"id":"pump-fun"}}}}]});
        assert_eq!(
            parse_pump_pool_addresses(&data),
            Err(RpcError::InvalidResponse)
        );
    }
    #[test]
    fn unrelated_dex_does_not_become_pumpfun_pool() {
        let data = json!({"data":[{"id":format!("solana_{ADDR}"),
            "attributes":{"address":ADDR},
            "relationships":{"dex":{"data":{"id":"raydium"}}}}]});
        assert!(parse_pump_pool_addresses(&data).unwrap().is_empty());
    }
}
