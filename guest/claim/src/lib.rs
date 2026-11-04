//! The claim that the zkVM program commits: what one solc run produced for one contract.
//! `ProvenanceRegistry` (contracts/src) decodes the same struct.

use alloy_primitives::keccak256;
use alloy_sol_types::sol;
use serde_json::Value;

sol! {
    /// A byte range in the runtime code that holds an immutable value.
    #[derive(Debug, PartialEq, Eq)]
    struct ImmutableRange {
        uint32 start;
        uint32 length;
    }

    /// Public values of the proof. The registry contract decodes the same struct.
    #[derive(Debug, PartialEq, Eq)]
    struct CompilationClaim {
        bytes32 inputHash;
        string compilerVersion;
        string contractName;
        bytes32 creationCodeHash;
        bytes32 runtimeCodeHash;
        uint32 runtimeCodeLength;
        ImmutableRange[] immutables;
        bytes32 metadataHash;
    }
}

/// Builds the claim for `contract` (`path:Name`) from standard JSON input and output bytes.
pub fn claim_from_output(
    input: &[u8],
    output: &[u8],
    contract: &str,
) -> Result<CompilationClaim, String> {
    let out: Value = serde_json::from_slice(output).map_err(|e| format!("output JSON: {e}"))?;
    if let Some(errors) = out.get("errors").and_then(Value::as_array) {
        let fatal: Vec<&str> = errors
            .iter()
            .filter(|e| e.get("severity").and_then(Value::as_str) == Some("error"))
            .filter_map(|e| e.get("formattedMessage").or(e.get("message")).and_then(Value::as_str))
            .collect();
        if !fatal.is_empty() {
            return Err(format!("compiler errors: {}", fatal.join("\n")));
        }
    }

    let (path, name) = contract.rsplit_once(':').ok_or("contract must be path:Name")?;
    let c = out
        .pointer(&format!("/contracts/{}/{}", escape(path), escape(name)))
        .ok_or_else(|| format!("contract {contract} not in output"))?;

    for ptr in ["/evm/bytecode/linkReferences", "/evm/deployedBytecode/linkReferences"] {
        if c.pointer(ptr).and_then(Value::as_object).is_some_and(|m| !m.is_empty()) {
            return Err("library linking is not supported".into());
        }
    }

    let creation = hex_field(c, "/evm/bytecode/object")?;
    let mut runtime = hex_field(c, "/evm/deployedBytecode/object")?;
    let immutables = immutable_ranges(c.pointer("/evm/deployedBytecode/immutableReferences"))?;
    zero_ranges(&mut runtime, &immutables)?;

    let metadata = c.get("metadata").and_then(Value::as_str).ok_or("missing metadata")?;
    let meta: Value = serde_json::from_str(metadata).map_err(|e| format!("metadata JSON: {e}"))?;
    let compiler_version =
        meta.pointer("/compiler/version").and_then(Value::as_str).ok_or("missing compiler.version")?;

    Ok(CompilationClaim {
        inputHash: keccak256(input),
        compilerVersion: compiler_version.to_string(),
        contractName: contract.to_string(),
        creationCodeHash: keccak256(&creation),
        runtimeCodeHash: keccak256(&runtime),
        runtimeCodeLength: u32::try_from(runtime.len()).map_err(|_| "runtime code too long")?,
        immutables,
        metadataHash: keccak256(metadata.as_bytes()),
    })
}

/// Checks onchain runtime code against a claim, the same way the registry contract does.
pub fn runtime_matches(onchain: &[u8], claim: &CompilationClaim) -> bool {
    if onchain.len() != claim.runtimeCodeLength as usize {
        return false;
    }
    let mut code = onchain.to_vec();
    zero_ranges(&mut code, &claim.immutables).is_ok() && keccak256(&code) == claim.runtimeCodeHash
}

/// Sets every immutable range to zero.
pub fn zero_ranges(code: &mut [u8], ranges: &[ImmutableRange]) -> Result<(), String> {
    for r in ranges {
        let (start, end) = (r.start as usize, r.start as usize + r.length as usize);
        code.get_mut(start..end).ok_or("immutable range out of bounds")?.fill(0);
    }
    Ok(())
}

fn immutable_ranges(v: Option<&Value>) -> Result<Vec<ImmutableRange>, String> {
    let mut ranges = Vec::new();
    let Some(map) = v.and_then(Value::as_object) else { return Ok(ranges) };
    for refs in map.values() {
        for r in refs.as_array().ok_or("immutableReferences entry is not an array")? {
            let field = |k: &str| {
                r.get(k).and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or("bad range")
            };
            ranges.push(ImmutableRange { start: field("start")?, length: field("length")? });
        }
    }
    // Same order for every compiler run, whatever the JSON map order.
    ranges.sort_by_key(|r| (r.start, r.length));
    Ok(ranges)
}

fn hex_field(c: &Value, ptr: &str) -> Result<Vec<u8>, String> {
    let s = c.pointer(ptr).and_then(Value::as_str).ok_or_else(|| format!("missing {ptr}"))?;
    hex::decode(s.trim_start_matches("0x")).map_err(|e| format!("{ptr}: {e}"))
}

// JSON pointer escaping (RFC 6901).
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUTPUT: &str = r#"{"contracts":{"src/A.sol":{"A":{
        "metadata":"{\"compiler\":{\"version\":\"0.8.38+commit.1835d247\"}}",
        "evm":{"bytecode":{"object":"6001","linkReferences":{}},
               "deployedBytecode":{"object":"60aa000000005b","linkReferences":{},
                 "immutableReferences":{"7":[{"start":2,"length":4}]}}}}}}}"#;

    #[test]
    fn claim_and_match() {
        let claim = claim_from_output(b"{}", OUTPUT.as_bytes(), "src/A.sol:A").unwrap();
        assert_eq!(claim.compilerVersion, "0.8.38+commit.1835d247");
        assert_eq!(claim.runtimeCodeLength, 7);
        assert_eq!(claim.immutables, vec![ImmutableRange { start: 2, length: 4 }]);
        // Onchain code has the immutable value filled in.
        assert!(runtime_matches(&hex::decode("60aa123456785b").unwrap(), &claim));
        assert!(!runtime_matches(&hex::decode("60ab123456785b").unwrap(), &claim));
    }

    #[test]
    fn rejects_compiler_errors() {
        let out = r#"{"errors":[{"severity":"error","formattedMessage":"boom"}]}"#;
        assert!(claim_from_output(b"{}", out.as_bytes(), "a:A").unwrap_err().contains("boom"));
    }
}
