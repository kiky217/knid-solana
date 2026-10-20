//! Fail-closed classification of Pump.fun *candidate* swap transactions.
//! Confirmation of execution, pool membership, and amount attribution is separate.
use crate::rpc::RpcError;
use serde_json::Value;
pub const PUMPFUN_PROGRAM: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateSide {
    Buy,
    Sell,
}
pub fn pumpfun_candidate(response: &Value) -> Result<Option<CandidateSide>, RpcError> {
    let tx = response
        .get("result")
        .filter(|x| !x.is_null())
        .ok_or(RpcError::InvalidResponse)?;
    let meta = tx.get("meta").ok_or(RpcError::InvalidResponse)?;
    if !meta.get("err").is_some_and(Value::is_null) {
        return Ok(None);
    }
    let instructions = tx
        .pointer("/transaction/message/instructions")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    let inner = meta.get("innerInstructions").and_then(Value::as_array);
    let outer = instructions
        .iter()
        .any(|i| i.get("programId").and_then(Value::as_str) == Some(PUMPFUN_PROGRAM));
    let nested = inner.is_some_and(|groups| {
        groups.iter().any(|g| {
            g.get("instructions")
                .and_then(Value::as_array)
                .is_some_and(|v| {
                    v.iter().any(|i| {
                        i.get("programId").and_then(Value::as_str) == Some(PUMPFUN_PROGRAM)
                    })
                })
        })
    });
    if !outer && !nested {
        return Ok(None);
    }
    let logs = meta
        .get("logMessages")
        .and_then(Value::as_array)
        .ok_or(RpcError::InvalidResponse)?;
    let buys = logs
        .iter()
        .filter(|x| x.as_str() == Some("Program log: Instruction: Buy"))
        .count();
    let sells = logs
        .iter()
        .filter(|x| x.as_str() == Some("Program log: Instruction: Sell"))
        .count();
    // Multiple sides or multiple same-side log entries may be routed/batched swaps:
    // cannot map a single event to a specific instruction without deeper decoding.
    match (buys, sells) {
        (1, 0) => Ok(Some(CandidateSide::Buy)),
        (0, 1) => Ok(Some(CandidateSide::Sell)),
        _ => Ok(None),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const P: &str = PUMPFUN_PROGRAM;
    #[test]
    fn detects_direct_candidate_not_proof() {
        let tx = serde_json::json!({"result":{"transaction":{"message":{"instructions":[{"programId":P}]}},
            "meta":{"err":null,"innerInstructions":[],"logMessages":["Program log: Instruction: Buy"]}}});
        assert_eq!(pumpfun_candidate(&tx), Ok(Some(CandidateSide::Buy)));
    }
    #[test]
    fn no_program_no_candidate() {
        let tx = serde_json::json!({"result":{"transaction":{"message":{"instructions":[{"programId":"11111111111111111111111111111111"}]}},
            "meta":{"err":null,"innerInstructions":[],"logMessages":["Program log: Instruction: Buy"]}}});
        assert_eq!(pumpfun_candidate(&tx), Ok(None));
    }
    #[test]
    fn conflicting_logs_fail_closed() {
        let tx = serde_json::json!({"result":{"transaction":{"message":{"instructions":[{"programId":P}]}},
            "meta":{"err":null,"innerInstructions":[],"logMessages":["Program log: Instruction: Buy","Program log: Instruction: Sell"]}}});
        assert_eq!(pumpfun_candidate(&tx), Ok(None));
    }
    #[test]
    fn failed_transaction_not_trade() {
        let tx = serde_json::json!({"result":{"transaction":{"message":{"instructions":[{"programId":P}]}},
            "meta":{"err":{"InstructionError":[0,"Custom"]},"innerInstructions":[],"logMessages":["Program log: Instruction: Sell"]}}});
        assert_eq!(pumpfun_candidate(&tx), Ok(None));
    }
}
