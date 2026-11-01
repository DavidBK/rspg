//! Formatting a single cell value for the detail popup and the editor.

use serde_json::Value;

/// A cell value prepared for display.
pub struct Formatted {
    pub text: String,
    pub is_json: bool,
    /// Something worth telling the user (e.g. why JSON didn't parse).
    pub note: Option<String>,
}

/// Pretty-print JSON the way `jq .` does (2-space indent); anything else is
/// returned unchanged.
pub fn format(value: &str) -> Formatted {
    let trimmed = value.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return Formatted { text: value.to_string(), is_json: false, note: None };
    }

    // `::<Value>` is the "turbofish": it tells `from_str` which type to
    // build. `Value` is serde_json's enum for "any JSON".
    match serde_json::from_str::<Value>(trimmed) {
        Ok(json) => Formatted {
            // Serializing a `Value` can't fail, so `expect` is fine here.
            text: serde_json::to_string_pretty(&json).expect("Value always serializes"),
            is_json: true,
            note: None,
        },
        Err(e) => Formatted {
            text: value.to_string(),
            is_json: false,
            note: Some(format!("invalid JSON: {e} (truncated? see pgcli max_field_width)")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretty_prints_like_jq() {
        let f = format(r#"{"b": 1, "a": [true, null], "c": {}}"#);
        assert!(f.is_json);
        // Key order is kept (`preserve_order` feature), not sorted.
        assert_eq!(f.text, "{\n  \"b\": 1,\n  \"a\": [\n    true,\n    null\n  ],\n  \"c\": {}\n}");
    }

    #[test]
    fn keeps_big_numbers_exact() {
        // Without `arbitrary_precision` this would become 1.2345678901234567e19.
        let f = format("[12345678901234567890, 0.1000000000000000055]");
        assert!(f.text.contains("12345678901234567890"));
        assert!(f.text.contains("0.1000000000000000055"));
    }

    #[test]
    fn plain_text_is_unchanged() {
        let f = format("hello");
        assert!(!f.is_json);
        assert_eq!(f.text, "hello");
        assert!(f.note.is_none());
    }

    #[test]
    fn truncated_json_gets_a_note() {
        let f = format(r#"{"a": [1, 2"#);
        assert!(!f.is_json);
        assert!(f.note.unwrap().starts_with("invalid JSON"));
    }
}
