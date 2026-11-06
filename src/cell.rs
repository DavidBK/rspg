//! Formatting a single cell value for the detail popup and the editor.

use serde_json::Value;

/// A cell value prepared for display.
pub struct Formatted {
    pub text: String,
    pub is_json: bool,
    /// Something worth telling the user (e.g. why JSON didn't parse).
    pub note: Option<String>,
}

/// Pretty-print JSON the way `jq .` does (2-space indent). Python-style
/// lists/dicts (how pgcli shows Postgres arrays, e.g. `['a', 'b']`) are
/// converted to JSON first. Anything else is returned unchanged.
pub fn format(value: &str) -> Formatted {
    let trimmed = value.trim();
    if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
        return Formatted { text: value.to_string(), is_json: false, note: None };
    }

    // `::<Value>` is the "turbofish": it tells `from_str` which type to
    // build. `Value` is serde_json's enum for "any JSON".
    let parsed = serde_json::from_str::<Value>(trimmed).or_else(|e| {
        // Not JSON: maybe a Python literal. If that fails too, report the
        // original JSON error (it's the more useful one).
        python_to_json(trimmed)
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .ok_or(e)
    });
    match parsed {
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

/// Rewrite a Python literal (`['a', None, {'k': True}]`) as JSON text:
/// quoted strings become JSON strings, `None`/`True`/`False` become
/// `null`/`true`/`false`, everything else is copied as is.
/// Returns `None` on an unterminated string.
fn python_to_json(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    // `peekable` lets us look at the next char without consuming it.
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                let text = read_python_string(&mut chars, c)?;
                // serde_json does the JSON quoting and escaping for us.
                out.push_str(&serde_json::to_string(&text).ok()?);
            }
            c if c.is_ascii_alphabetic() => {
                let mut word = String::from(c);
                while let Some(&n) = chars.peek()
                    && n.is_ascii_alphanumeric()
                {
                    word.push(n);
                    chars.next();
                }
                out.push_str(match word.as_str() {
                    "None" => "null",
                    "True" => "true",
                    "False" => "false",
                    other => other, // JSON parsing will reject it
                });
            }
            c => out.push(c),
        }
    }
    Some(out)
}

/// Read a Python string body up to the closing `quote` and decode its
/// escapes (`\'`, `\n`, `\x41`, `\u00e9`, ...).
fn read_python_string(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    quote: char,
) -> Option<String> {
    let mut text = String::new();
    loop {
        match chars.next()? {
            c if c == quote => return Some(text),
            '\\' => {
                let e = chars.next()?;
                // Hex escapes: read `len` hex digits into a char.
                let mut hex = |len: usize| -> Option<char> {
                    let digits: String = chars.by_ref().take(len).collect();
                    char::from_u32(u32::from_str_radix(&digits, 16).ok()?)
                };
                match e {
                    'n' => text.push('\n'),
                    't' => text.push('\t'),
                    'r' => text.push('\r'),
                    'x' => text.push(hex(2)?),
                    'u' => text.push(hex(4)?),
                    'U' => text.push(hex(8)?),
                    '\\' | '\'' | '"' => text.push(e),
                    other => {
                        text.push('\\');
                        text.push(other);
                    }
                }
            }
            c => text.push(c),
        }
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
    fn python_list_becomes_json() {
        let f = format(r#"['a', "it's", 'say \'hi\'', None, True, 1.5, {'k': False}]"#);
        assert!(f.is_json, "{:?}", f.note);
        let v: Value = serde_json::from_str(&f.text).unwrap();
        assert_eq!(v, serde_json::json!(["a", "it's", "say 'hi'", null, true, 1.5, {"k": false}]));
    }

    #[test]
    fn python_escapes() {
        let f = format(r#"['a\nb', '\xe9', 'back\\slash', 'q"']"#);
        let v: Value = serde_json::from_str(&f.text).unwrap();
        assert_eq!(v, serde_json::json!(["a\nb", "é", "back\\slash", "q\""]));
    }

    #[test]
    fn truncated_json_gets_a_note() {
        let f = format(r#"{"a": [1, 2"#);
        assert!(!f.is_json);
        assert!(f.note.unwrap().starts_with("invalid JSON"));
    }
}
