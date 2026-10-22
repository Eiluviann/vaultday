//! Read-only view of a note's YAML frontmatter. The block is parsed once with
//! `serde_norway` and exposed as JSON values; the writer never re-serialises
//! YAML, it replaces lines, so the raw text stays the source of truth.

use serde_json::{Map, Value};

/// The parsed frontmatter of a note. Keys keep their source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Frontmatter {
    raw: String,
    fields: Map<String, Value>,
    parse_error: Option<String>,
}

impl Frontmatter {
    /// Parses the YAML between the `---` delimiters (delimiters excluded).
    /// A block that is not valid YAML or not a mapping yields no fields and a
    /// recorded error; the note itself still parses and round-trips.
    pub(crate) fn parse(raw: &str) -> Frontmatter {
        let (fields, parse_error) = match parse_mapping(raw) {
            Ok(fields) => (fields, None),
            Err(message) => (Map::new(), Some(message)),
        };
        Frontmatter {
            raw: raw.to_owned(),
            fields,
            parse_error,
        }
    }

    /// The YAML text between the delimiters, line endings normalised to `\n`.
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Every top-level key in source order.
    #[must_use]
    pub fn fields(&self) -> &Map<String, Value> {
        &self.fields
    }

    /// Why the block could not be read as a YAML mapping, when it could not.
    #[must_use]
    pub fn parse_error(&self) -> Option<&str> {
        self.parse_error.as_deref()
    }

    /// True when the key exists, whatever its value.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    /// The raw value of a key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// A key's value as text: strings as written, numbers and booleans
    /// formatted, `null` and missing keys as `None`, lists and maps as `None`.
    #[must_use]
    pub fn scalar(&self, key: &str) -> Option<String> {
        scalar_text(self.fields.get(key)?)
    }

    /// A key's value as a list of texts: list items that are scalars, or a
    /// single scalar as a one-item list. Missing, `null` and map values are empty.
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.fields.get(key) {
            Some(Value::Array(items)) => items.iter().filter_map(scalar_text).collect(),
            Some(value) => scalar_text(value).into_iter().collect(),
            None => Vec::new(),
        }
    }

    /// A key's value as an integer when it is one, or a string holding one.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        match self.fields.get(key)? {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    }
}

fn parse_mapping(raw: &str) -> Result<Map<String, Value>, String> {
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let yaml: serde_norway::Value = serde_norway::from_str(raw)
        .map_err(|error| format!("frontmatter is not valid YAML: {error}"))?;
    let json = serde_json::to_value(yaml)
        .map_err(|error| format!("frontmatter has a value JSON cannot hold: {error}"))?;
    match json {
        Value::Object(fields) => Ok(fields),
        Value::Null => Ok(Map::new()),
        other => Err(format!(
            "frontmatter must be a mapping, found {}",
            value_kind(&other)
        )),
    }
}

fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "a mapping",
    }
}

#[cfg(test)]
mod tests {
    use super::Frontmatter;

    #[test]
    fn reads_scalars_lists_and_keeps_order() {
        let frontmatter = Frontmatter::parse(
            "type: story\nnumber: WEB-142\nsort_index: 1000\ndue: 2026-10-30\naliases:\n  - Checkout\n  - Cart v2\ntags: [a, b]\nempty:\n",
        );
        assert!(frontmatter.parse_error().is_none());
        assert_eq!(
            frontmatter.fields().keys().collect::<Vec<_>>(),
            [
                "type",
                "number",
                "sort_index",
                "due",
                "aliases",
                "tags",
                "empty"
            ]
        );
        assert_eq!(frontmatter.scalar("number").as_deref(), Some("WEB-142"));
        assert_eq!(frontmatter.scalar("sort_index").as_deref(), Some("1000"));
        assert_eq!(frontmatter.integer("sort_index"), Some(1000));
        assert_eq!(frontmatter.scalar("due").as_deref(), Some("2026-10-30"));
        assert_eq!(frontmatter.list("aliases"), ["Checkout", "Cart v2"]);
        assert_eq!(frontmatter.list("tags"), ["a", "b"]);
        assert_eq!(frontmatter.list("number"), ["WEB-142"]);
        assert!(frontmatter.has("empty"));
        assert_eq!(frontmatter.scalar("empty"), None);
        assert!(frontmatter.list("empty").is_empty());
        assert_eq!(frontmatter.scalar("missing"), None);
    }

    #[test]
    fn quoted_values_lose_their_quotes_and_bad_yaml_is_reported() {
        let frontmatter =
            Frontmatter::parse("ticket: \"[Open](https://example.test/x)\"\nflag: true\n");
        assert_eq!(
            frontmatter.scalar("ticket").as_deref(),
            Some("[Open](https://example.test/x)")
        );
        assert_eq!(frontmatter.scalar("flag").as_deref(), Some("true"));

        let broken = Frontmatter::parse("key: [unclosed\n");
        assert!(broken.parse_error().is_some());
        assert!(broken.fields().is_empty());

        let list = Frontmatter::parse("- a\n- b\n");
        assert_eq!(
            list.parse_error(),
            Some("frontmatter must be a mapping, found a list")
        );

        assert!(Frontmatter::parse("").parse_error().is_none());
    }
}
