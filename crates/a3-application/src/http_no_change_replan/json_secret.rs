//! Inspect bounded decoded JSON without losing overwritten duplicate object values.
use a3_domain::SecretCandidateClassifierV1;
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::fmt;

/// Inspection could not safely cover the entire bounded JSON document.
/// This is not evidence that the document contains a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsafeInspection;

/// Retains only a classification; parse/resource failures never become a safe result.
pub(super) fn contains_secret(bytes: &[u8]) -> Result<bool, UnsafeInspection> {
    if bytes.len() > super::MAX_REPLY_BYTES {
        return Err(UnsafeInspection);
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let found = (Scan { key: None })
        .deserialize(&mut decoder)
        .map_err(|_| UnsafeInspection)?;
    decoder.end().map_err(|_| UnsafeInspection)?;
    Ok(found)
}

struct Scan<'a> {
    key: Option<&'a str>,
}
impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = bool;
    fn deserialize<D>(self, deserializer: D) -> Result<bool, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Scan<'_> {
    type Value = bool;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a bounded JSON value")
    }
    fn visit_unit<E>(self) -> Result<bool, E> {
        Ok(false)
    }
    fn visit_bool<E>(self, _: bool) -> Result<bool, E> {
        Ok(false)
    }
    fn visit_i64<E>(self, _: i64) -> Result<bool, E> {
        Ok(false)
    }
    fn visit_u64<E>(self, _: u64) -> Result<bool, E> {
        Ok(false)
    }
    fn visit_f64<E>(self, _: f64) -> Result<bool, E> {
        Ok(false)
    }
    fn visit_str<E>(self, text: &str) -> Result<bool, E> {
        Ok(SecretCandidateClassifierV1::classify(text).is_some()
            || self.key.is_some_and(|key| {
                SecretCandidateClassifierV1::classify(&format!("{key}={text}")).is_some()
            }))
    }
    fn visit_seq<A>(self, mut sequence: A) -> Result<bool, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut found = false;
        while let Some(child) = sequence.next_element_seed(Scan { key: None })? {
            found |= child;
        }
        Ok(found)
    }
    fn visit_map<A>(self, mut map: A) -> Result<bool, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut found = false;
        while let Some(key) = map.next_key::<String>()? {
            found |= SecretCandidateClassifierV1::classify(&key).is_some();
            // Always consume every entry, including duplicate keys and later syntax errors.
            found |= map.next_value_seed(Scan { key: Some(&key) })?;
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::{UnsafeInspection, contains_secret};

    #[test]
    fn decoded_escaped_keys_nested_values_and_duplicate_first_values_are_inspected() {
        for bytes in [
            br#"{"x":"\u0041KIAIOSFODNN7EXAMPLE","x":"safe"}"#.as_slice(),
            br#"{"\u0041KIAIOSFODNN7EXAMPLE":false}"#.as_slice(),
            br#"{"x":[0,true,null,{"nested":"password=fixture-secret-value"}]}"#.as_slice(),
            br#"{"x":{"password":"fixture-secret-value"}}"#.as_slice(),
            br#"{"x":{"pass\u0077ord":"fixture-secret-value"}}"#.as_slice(),
        ] {
            assert_eq!(contains_secret(bytes), Ok(true));
        }
    }

    #[test]
    fn incomplete_invalid_or_oversize_json_is_uninspectable_without_claiming_a_secret() {
        for bytes in [
            br#"{"x":"AKIAIOSFODNN7EXAMPLE""#.as_slice(),
            br#"{"x":"AKIAIOSFODNN7EXAMPLE"} trailing"#.as_slice(),
            br#"{"x":"AKIAIOSFODNN7EXAMPLE",}"#.as_slice(),
        ] {
            assert_eq!(contains_secret(bytes), Err(UnsafeInspection));
        }
        assert_eq!(
            contains_secret(&vec![b' '; super::super::MAX_REPLY_BYTES + 1]),
            Err(UnsafeInspection)
        );
    }

    #[test]
    fn bounded_deep_unknown_or_duplicate_json_is_uninspectable_even_after_a_secret() {
        let deep = format!("{}0{}", "[".repeat(130), "]".repeat(130));
        for reply in [
            format!(r#"{{"early":"\u0041KIAIOSFODNN7EXAMPLE","pad":{deep}}}"#),
            format!(r#"{{"x":"\u0041KIAIOSFODNN7EXAMPLE","x":"safe","pad":{deep}}}"#),
            format!(r#"{{"pad":{deep},"late":"\u0041KIAIOSFODNN7EXAMPLE"}}"#),
            format!(r#"{{"pad":{deep},"late":"ordinary"}}"#),
        ] {
            assert!(reply.len() <= super::super::MAX_REPLY_BYTES);
            assert_eq!(contains_secret(reply.as_bytes()), Err(UnsafeInspection));
        }
    }

    #[test]
    fn ordinary_bounded_json_values_remain_structural_decoder_input() {
        for bytes in [
            br#"{"halt":"needEvidence"}"#.as_slice(),
            br#"{"first":2,"last":3,"intent":"Change the bounded part"}"#.as_slice(),
            br#"{"x":[-1,1.5,true,false,null,"the Kakia module"]}"#.as_slice(),
            br#"{"password":"short"}"#.as_slice(),
        ] {
            assert_eq!(contains_secret(bytes), Ok(false));
        }
    }
}
