//! Strict parsing shared in behavior with the CLI; reject ambiguous or lossy authored JSON.
use crate::DocumentError;
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::Value;
type Result<T> = std::result::Result<T, DocumentError>;
fn failure(_code: &str, message: impl std::fmt::Display) -> DocumentError {
    DocumentError(message.to_string())
}
pub(crate) fn load_value(source: &str) -> Result<Value> {
    reject_oversized_integers(source)?;
    Ok(serde_json::from_str::<UniqueValue>(source)?.0)
}
// Preserve integer safety before Serde can round out-of-range integer tokens.
fn reject_oversized_integers(source: &str) -> Result<()> {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || matches!(bytes[i], b'.' | b'e' | b'E' | b'+' | b'-'))
            {
                i += 1;
            }
            let token = &source[start..i];
            if !token.contains(['.', 'e', 'E']) {
                let fits = if token.starts_with('-') {
                    token.parse::<i64>().is_ok()
                } else {
                    token.parse::<u64>().is_ok()
                };
                if !fits {
                    return Err(failure(
                        "data_loss",
                        "Integer token exceeds supported 64-bit range",
                    ));
                }
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

// Reject duplicate members before serde_json::Value can discard them. This also
// covers nested objects and patches, not only the top-level project.
struct UniqueValue(Value);

impl<'de> Deserialize<'de> for UniqueValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON with unique object members")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(value.into()))
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|v| UniqueValue(Value::Number(v)))
                    .ok_or_else(|| E::custom("Nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.to_owned())))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("Duplicate JSON member: {key}")));
                    }
                    values.insert(key, map.next_value::<UniqueValue>()?.0);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

// Serde models may ignore unknown fields. Reject them rather than silently
// discarding authored data during apply or export. Missing/default fields are OK.
pub(crate) fn check_preserved(input: &Value, canonical: &Value, path: &str) -> Result<()> {
    match (input, canonical) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                let other = b.get(key).ok_or_else(|| {
                    failure("unknown_field", format!("Unsupported field at {child}"))
                })?;
                check_preserved(value, other, &child)?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(failure("data_loss", format!("Array changed at {path}")));
            }
            for (index, (value, other)) in a.iter().zip(b).enumerate() {
                check_preserved(value, other, &format!("{path}/{index}"))?;
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            let equivalent = if a == b {
                true
            } else if let Some(integer) = a
                .as_i64()
                .map(i128::from)
                .or_else(|| a.as_u64().map(i128::from))
            {
                b.as_f64()
                    .is_some_and(|number| number.fract() == 0.0 && number as i128 == integer)
            } else {
                a.as_f64() == b.as_f64()
            };
            if !equivalent {
                return Err(failure(
                    "data_loss",
                    format!("Number cannot be represented exactly at {path}"),
                ));
            }
        }
        _ if input != canonical => {
            return Err(failure("data_loss", format!("Value changed at {path}")));
        }
        _ => {}
    }
    Ok(())
}
