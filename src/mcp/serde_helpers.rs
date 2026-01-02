// ABOUTME: Custom serde deserializers for flexible parameter handling.
// ABOUTME: Allows MCP tools to accept numeric parameters as either integers or strings.

use serde::{de, Deserializer};
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;

struct FlexibleU32Visitor;

impl<'de> de::Visitor<'de> for FlexibleU32Visitor {
    type Value = u32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a u32 integer or a string representation of a u32")
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value).map_err(|_| E::custom(format!("u32 out of range: {}", value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value).map_err(|_| E::custom(format!("u32 out of range: {}", value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::from_str(value).map_err(|_| E::custom(format!("invalid u32 string: {}", value)))
    }
}

/// Deserializes a u32 from either an integer or string representation.
pub fn deserialize_flexible_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(FlexibleU32Visitor)
}

struct FlexibleOptionVisitor<T> {
    _marker: PhantomData<T>,
}

impl<T> FlexibleOptionVisitor<T> {
    fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<'de> de::Visitor<'de> for FlexibleOptionVisitor<u32> {
    type Value = Option<u32>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("null, a u32 integer, or a string representation of a u32")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_flexible_u32(deserializer).map(Some)
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom(format!("u32 out of range: {}", value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::try_from(value)
            .map(Some)
            .map_err(|_| E::custom(format!("u32 out of range: {}", value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        u32::from_str(value)
            .map(Some)
            .map_err(|_| E::custom(format!("invalid u32 string: {}", value)))
    }
}

/// Deserializes an Option<u32> from null, an integer, or a string representation.
pub fn deserialize_flexible_u32_opt<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(FlexibleOptionVisitor::<u32>::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestParams {
        #[serde(deserialize_with = "deserialize_flexible_u32")]
        line: u32,
        #[serde(default, deserialize_with = "deserialize_flexible_u32_opt")]
        character: Option<u32>,
    }

    #[test]
    fn deserialize_u32_from_integer() {
        let json = r#"{"line": 56, "character": 15}"#;
        let params: TestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.line, 56, "line should deserialize from integer");
        assert_eq!(params.character, Some(15), "character should deserialize from integer");
    }

    #[test]
    fn deserialize_u32_from_string() {
        let json = r#"{"line": "56", "character": "15"}"#;
        let params: TestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.line, 56, "line should deserialize from string");
        assert_eq!(params.character, Some(15), "character should deserialize from string");
    }

    #[test]
    fn deserialize_optional_u32_from_null() {
        let json = r#"{"line": 1, "character": null}"#;
        let params: TestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.character, None, "character should be None when null");
    }

    #[test]
    fn deserialize_optional_u32_missing_field() {
        let json = r#"{"line": 1}"#;
        let params: TestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.character, None, "character should be None when missing");
    }

    #[test]
    fn deserialize_u32_rejects_invalid_string() {
        let json = r#"{"line": "abc"}"#;
        let result: Result<TestParams, _> = serde_json::from_str(json);
        assert!(result.is_err(), "should reject non-numeric string");
    }

    #[test]
    fn deserialize_u32_rejects_negative() {
        let json = r#"{"line": -5}"#;
        let result: Result<TestParams, _> = serde_json::from_str(json);
        assert!(result.is_err(), "should reject negative number");
    }
}
