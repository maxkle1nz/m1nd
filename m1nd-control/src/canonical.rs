use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Version pinned into contracts that depend on deterministic JSON bytes.
pub const CANONICALIZATION_VERSION: &str = "m1nd-canonical-json-v1";

const DIGEST_PREFIX: &[u8] = b"m1nd-domain-separated-sha256-v1\0";

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error("canonical JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),
}

/// Serialize a value into deterministic RFC 8785-like JSON bytes.
///
/// Object keys are sorted recursively. Arrays retain their declared order.
/// Numbers and strings use `serde_json`'s stable wire encoding, intentionally
/// avoiding locale-dependent or debug formatting.
pub fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let value = serde_json::to_value(value)?;
    let mut output = Vec::new();
    write_value(&value, &mut output)?;
    Ok(output)
}

pub fn canonical_json_string<T: Serialize + ?Sized>(value: &T) -> Result<String, CanonicalError> {
    let bytes = canonical_json(value)?;
    // Canonical JSON is always UTF-8 because it is emitted by serde_json.
    Ok(String::from_utf8(bytes).expect("serde_json emitted non-UTF-8 JSON"))
}

fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), serde_json::Error> {
    match value {
        Value::Null => output.extend_from_slice(b"null"),
        Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
        Value::Number(number) => serde_json::to_writer(output, number)?,
        Value::String(string) => serde_json::to_writer(output, string)?,
        Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_value(value, output)?;
            }
            output.push(b']');
        }
        Value::Object(object) => {
            output.push(b'{');
            let mut keys: Vec<&String> = object.keys().collect();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                serde_json::to_writer(&mut *output, key)?;
                output.push(b':');
                write_value(&object[key], output)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

/// SHA-256 with an unambiguous domain prefix and length-delimited domain name.
pub fn digest_domain_bytes(domain: &str, payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DIGEST_PREFIX);
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain.as_bytes());
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    hex_lower(&hasher.finalize())
}

pub fn digest_canonical<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, CanonicalError> {
    Ok(digest_domain_bytes(domain, &canonical_json(value)?))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    #[test]
    fn recursive_object_keys_are_sorted_and_arrays_keep_order() {
        let input = json!({
            "z": [{"b": 2, "a": 1}, {"d": 4, "c": 3}],
            "a": {"y": "second", "x": "first"}
        });

        assert_eq!(
            canonical_json_string(&input).unwrap(),
            r#"{"a":{"x":"first","y":"second"},"z":[{"a":1,"b":2},{"c":3,"d":4}]}"#
        );
    }

    #[test]
    fn insertion_order_does_not_change_bytes_or_digest() {
        let mut left = Map::new();
        left.insert("b".into(), json!(2));
        left.insert("a".into(), json!({"d": 4, "c": 3}));
        let mut right = Map::new();
        right.insert("a".into(), json!({"c": 3, "d": 4}));
        right.insert("b".into(), json!(2));

        let left = Value::Object(left);
        let right = Value::Object(right);
        assert_eq!(
            canonical_json(&left).unwrap(),
            canonical_json(&right).unwrap()
        );
        assert_eq!(
            digest_canonical("ordering-test", &left).unwrap(),
            digest_canonical("ordering-test", &right).unwrap()
        );
    }

    #[test]
    fn serde_json_number_and_string_encoding_is_preserved() {
        let value = json!({"float": 1.0, "line": "a\nb\"c", "whole": 1});
        let canonical = canonical_json_string(&value).unwrap();
        assert_eq!(canonical, serde_json::to_string(&value).unwrap());
    }

    #[test]
    fn changed_payload_byte_and_changed_domain_change_digest() {
        let first = digest_domain_bytes("m1nd-test-a", b"payload-a");
        let changed_byte = digest_domain_bytes("m1nd-test-a", b"payload-b");
        let changed_domain = digest_domain_bytes("m1nd-test-b", b"payload-a");

        assert_ne!(first, changed_byte);
        assert_ne!(first, changed_domain);
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
