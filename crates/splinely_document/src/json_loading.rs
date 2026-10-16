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

/// Lazily rendered JSON-pointer-like location: segments are only turned into a
/// string when an error is reported, so the success path never allocates.
#[derive(Clone, Copy)]
struct Location<'p> {
    parent: Option<&'p Location<'p>>,
    key: Option<&'p str>,
    index: usize,
}

fn render(base: &str, location: Option<&Location<'_>>) -> String {
    let mut segments = Vec::new();
    let mut cursor = location;
    while let Some(l) = cursor {
        segments.push(l);
        cursor = l.parent;
    }
    let mut out = base.to_owned();
    for l in segments.into_iter().rev() {
        out.push('/');
        match l.key {
            Some(key) => out.push_str(&key.replace('~', "~0").replace('/', "~1")),
            None => out.push_str(&l.index.to_string()),
        }
    }
    out
}

/// The number-preservation rule shared by `check_preserved` and the fast path
/// in `tree.rs`: equal tokens, or an integer that the canonical float holds exactly.
/// Integer tokens compare exactly (never through `f64`), an integer input may
/// become a float holding exactly that integer (never `-0.0`), a float input
/// never matches an integer canonical value, and floats compare by bit pattern
/// so the sign of zero is preserved.
pub(crate) fn numbers_equivalent(a: &serde_json::Number, b: &serde_json::Number) -> bool {
    let integer = |n: &serde_json::Number| {
        n.as_i64()
            .map(i128::from)
            .or_else(|| n.as_u64().map(i128::from))
    };
    match (integer(a), integer(b)) {
        (Some(x), Some(y)) => x == y,
        (Some(x), None) => b.as_f64().is_some_and(|number| {
            number.fract() == 0.0
                && !(number == 0.0 && number.is_sign_negative())
                && number as i128 == x
        }),
        (None, Some(_)) => false,
        (None, None) => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.to_bits() == y.to_bits(),
            _ => false,
        },
    }
}

// Serde models may ignore unknown fields. Reject them rather than silently
// discarding authored data during apply or export. Missing/default fields are OK.
pub(crate) fn check_preserved(input: &Value, canonical: &Value, path: &str) -> Result<()> {
    check_preserved_coded(input, canonical, path).map_err(|e| DocumentError(e.message))
}

/// Same check, keeping the machine-readable failure code (`unknown_field` or `data_loss`).
pub(crate) fn check_preserved_coded(
    input: &Value,
    canonical: &Value,
    path: &str,
) -> std::result::Result<(), crate::StrictLoadError> {
    preserved(input, canonical, path, None)
}

fn coded(code: &'static str, message: impl std::fmt::Display) -> crate::StrictLoadError {
    crate::StrictLoadError {
        code,
        message: message.to_string(),
    }
}

fn preserved(
    input: &Value,
    canonical: &Value,
    base: &str,
    here: Option<&Location<'_>>,
) -> std::result::Result<(), crate::StrictLoadError> {
    match (input, canonical) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                let location = Location {
                    parent: here,
                    key: Some(key),
                    index: 0,
                };
                let other = b.get(key).ok_or_else(|| {
                    coded(
                        "unknown_field",
                        format!(
                            "Unsupported field at {}. If the file was made with a newer Splinely, update Splinely (this is {}).",
                            render(base, Some(&location)),
                            env!("CARGO_PKG_VERSION")
                        ),
                    )
                })?;
                preserved(value, other, base, Some(&location))?;
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Err(coded(
                    "data_loss",
                    format!("Array changed at {}", render(base, here)),
                ));
            }
            for (index, (value, other)) in a.iter().zip(b).enumerate() {
                let location = Location {
                    parent: here,
                    key: None,
                    index,
                };
                preserved(value, other, base, Some(&location))?;
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            if !numbers_equivalent(a, b) {
                return Err(coded(
                    "data_loss",
                    format!(
                        "Number cannot be represented exactly at {}",
                        render(base, here)
                    ),
                ));
            }
        }
        _ if input != canonical => {
            return Err(coded(
                "data_loss",
                format!("Value changed at {}", render(base, here)),
            ));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::numbers_equivalent;
    use serde_json::Number;

    fn n(text: &str) -> Number {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn integers_compare_exactly() {
        assert!(numbers_equivalent(&n("5"), &n("5")));
        assert!(!numbers_equivalent(
            &n("9007199254740993"),
            &n("9007199254740992")
        ));
        assert!(!numbers_equivalent(&n("-1"), &n("1")));
        assert!(numbers_equivalent(
            &n("18446744073709551615"),
            &n("18446744073709551615")
        ));
        assert!(!numbers_equivalent(&n("18446744073709551615"), &n("-1")));
    }

    #[test]
    fn integer_input_may_become_the_exact_float_only() {
        assert!(numbers_equivalent(&n("3"), &n("3.0")));
        assert!(numbers_equivalent(&n("0"), &n("0.0")));
        assert!(!numbers_equivalent(&n("0"), &n("-0.0")));
        // 2^53 + 1 is not representable: the float holds 2^53, so data would be lost.
        assert!(!numbers_equivalent(
            &n("9007199254740993"),
            &n("9007199254740992.0")
        ));
        assert!(!numbers_equivalent(&n("3"), &n("3.5")));
    }

    #[test]
    fn float_input_never_matches_an_integer_canonical_value() {
        assert!(!numbers_equivalent(&n("3.0"), &n("3")));
        assert!(!numbers_equivalent(&n("0.0"), &n("0")));
        assert!(!numbers_equivalent(
            &n("9007199254740992.0"),
            &n("9007199254740993")
        ));
    }

    #[test]
    fn floats_keep_the_sign_of_zero() {
        assert!(numbers_equivalent(&n("-0.0"), &n("-0.0")));
        assert!(numbers_equivalent(&n("0.0"), &n("0.0")));
        assert!(!numbers_equivalent(&n("-0.0"), &n("0.0")));
        assert!(!numbers_equivalent(&n("0.0"), &n("-0.0")));
        assert!(numbers_equivalent(&n("1.5e-7"), &n("1.5e-7")));
        assert!(!numbers_equivalent(&n("1.5"), &n("1.25")));
    }
}
