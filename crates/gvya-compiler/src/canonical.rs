//! Canonical byte encoding and hashing primitives owned by the compiler.
//!
//! Canonical JSON is used for transparent source/IR documents. Object keys are emitted in
//! lexicographic order, insignificant whitespace is absent, strings use JSON escaping, and
//! non-finite numbers are rejected before they can become artifact semantics.

use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalError {
    NonFiniteNumber,
    SerializeString(String),
}

pub fn canonical_json(value: &serde_json::Value) -> Result<Vec<u8>, CanonicalError> {
    let mut out = Vec::new();
    write_value(value, &mut out)?;
    Ok(out)
}

pub fn canonical_json_string(value: &serde_json::Value) -> Result<String, CanonicalError> {
    String::from_utf8(canonical_json(value)?)
        .map_err(|error| CanonicalError::SerializeString(error.to_string()))
}

fn write_value(value: &serde_json::Value, out: &mut Vec<u8>) -> Result<(), CanonicalError> {
    match value {
        serde_json::Value::Null => out.extend_from_slice(b"null"),
        serde_json::Value::Bool(value) => {
            out.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        serde_json::Value::Number(value) => {
            // serde_json itself cannot represent NaN/Infinity. Keep the explicit check for values
            // produced through future custom number adapters.
            if value.as_f64().is_some_and(|number| !number.is_finite()) {
                return Err(CanonicalError::NonFiniteNumber);
            }
            out.extend_from_slice(value.to_string().as_bytes());
        }
        serde_json::Value::String(value) => {
            let encoded = serde_json::to_string(value)
                .map_err(|error| CanonicalError::SerializeString(error.to_string()))?;
            out.extend_from_slice(encoded.as_bytes());
        }
        serde_json::Value::Array(values) => {
            out.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                write_value(value, out)?;
            }
            out.push(b']');
        }
        serde_json::Value::Object(values) => {
            out.push(b'{');
            let mut keys: Vec<&String> = values.keys().collect();
            keys.sort();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                let encoded = serde_json::to_string(key)
                    .map_err(|error| CanonicalError::SerializeString(error.to_string()))?;
                out.extend_from_slice(encoded.as_bytes());
                out.push(b':');
                write_value(&values[key], out)?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&sha256(bytes))
}

#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

pub fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = nibble(chunk[0])?;
        let low = nibble(chunk[1])?;
        out[index] = (high << 4) | low;
    }
    Some(out)
}

fn nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_sorts_object_keys_recursively() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"z":1,"a":{"y":2,"x":3}}"#).unwrap();
        assert_eq!(
            canonical_json_string(&value).unwrap(),
            r#"{"a":{"x":3,"y":2},"z":1}"#
        );
    }

    #[test]
    fn sha_hex_round_trip() {
        let digest = sha256(b"gvya");
        assert_eq!(decode_hex_32(&hex(&digest)), Some(digest));
    }
}
