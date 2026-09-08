//! Shared, versioned data-minimization policy. This detects recognizable credentials,
//! labelled secret values and environment dumps, not arbitrary unlabelled private data.
//! Rejection diagnostics never contain the matched key, value or surrounding text.
use crate::{Error, Result};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::{borrow::Cow, sync::LazyLock};

pub const SECRET_POLICY_VERSION: u32 = 2;
pub const SENSITIVE_CONTENT_WITHHELD: &str = "[sensitive content withheld]";
const REJECTION: &str =
    "sensitive content is not accepted; remove secret values or use explicit redacted placeholders";

static KNOWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(?:^|[^\p{L}\p{N}_])(?:sk-[a-z0-9_-]{16,}|gh[pousr]_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}|",
        r"xox[baprs]-[a-z0-9-]{12,}|\b(?:AKIA|ASIA)[A-Z0-9]{16}\b|",
        r"\beyJ[a-z0-9_-]{8,}\.[a-z0-9_-]{8,}\.[a-z0-9_-]{8,}|",
        r"-----BEGIN (?:[A-Z0-9]+ )?PRIVATE KEY-----|",
        r"\bbearer[\s]+[a-z0-9+/_=-]{8,}|",
        r"[a-z][a-z0-9+.-]*://[^\s/@:]+:[^\s/@]+@)"
    ))
    .expect("fixed credential pattern")
});

static BASIC_AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^\p{L}\p{N}_])basic[\s]+([a-z0-9+/_=-]+)")
        .expect("fixed Basic authentication pattern")
});

static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
    r"(?i)(?:^|[^\p{L}\p{N}_])(?:[a-z][a-z0-9_]*[_-])?",
    r"(?:api[\s_-]*key|access[\s_-]*token|refresh[\s_-]*token|id[\s_-]*token|",
    r"client[\s_-]*secret|password|passwd|pwd|secret|token|authorization|",
    r"private[\s_-]*prompt|密码|密碼|口令|令牌|密钥|密鑰|私有提示词|私有提示詞|私有[\s_-]*prompt)",
    r#"[\s\"'`]*[:=][\s]*"#
)).expect("fixed secret assignment pattern")
});

static ENV_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s;])(?:export[ \t]+)?[A-Z_][A-Z0-9_]{1,80}[ \t]*=[ \t]*")
        .expect("fixed environment assignment pattern")
});

static PRIVATE_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r"(?im)^[ \t]*(?:#{1,6}[ \t]+)?(?:private[ _-]*prompt|私有提示词|私有提示詞)[ \t]*[:=]?[ \t]*\r?$"
).expect("fixed private prompt heading pattern")
});

static ENV_BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    r#"(?i)(?:^|[^\p{L}\p{N}_])(?:env|environment|environment[ _-]*(?:variables|vars|dump)|环境变量|環境變數)[\s\"']*[:=]"#
).expect("fixed environment object pattern")
});

fn folded_char(c: char) -> Option<char> {
    match c {
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0),
        '\u{3000}' => Some(' '),
        '\u{200b}'..='\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2060}'..='\u{206f}'
        | '\u{feff}' => None,
        _ => Some(c),
    }
}

/// Only detection is normalized; stored public content is never rewritten.
/// Decode JSON unicode escapes as well, including those embedded in Markdown.
fn normalized(text: &str) -> Cow<'_, str> {
    if !text.contains('\\') && !text.chars().any(|c| folded_char(c) != Some(c)) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(mut c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'u') {
            let mut candidate = chars.clone();
            candidate.next();
            let hex: String = candidate.by_ref().take(4).collect();
            if hex.len() == 4
                && hex.bytes().all(|b| b.is_ascii_hexdigit())
                && let Ok(code) = u32::from_str_radix(&hex, 16)
                && let Some(decoded) = char::from_u32(code)
            {
                c = decoded;
                chars = candidate;
            }
        }
        if let Some(c) = folded_char(c) {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

fn placeholder(text: &str) -> bool {
    let text = text.trim();
    matches!(
        text.to_ascii_lowercase().as_str(),
        "" | "[redacted]" | "<redacted>" | "[withheld]" | "<withheld>" | "***"
    ) || (text.starts_with("${") && text.ends_with('}') && env_name(&text[2..text.len() - 1]))
        || (text.starts_with('$') && env_name(&text[1..]))
}
fn env_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !value.as_bytes()[0].is_ascii_digit()
}
fn has_value(rest: &str) -> bool {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return false;
    }
    if ["null", "~", "{}", "[]"].iter().any(|literal| {
        rest.strip_prefix(literal).is_some_and(|tail| {
            tail.is_empty()
                || tail
                    .starts_with(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '}' | ']'))
        })
    }) {
        return false;
    }
    let value = if let Some(quote) = rest
        .chars()
        .next()
        .filter(|c| matches!(c, '\'' | '"' | '`'))
    {
        let tail = &rest[quote.len_utf8()..];
        tail.split(quote).next().unwrap_or(tail)
    } else {
        rest.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '"' | '\'' | '`'))
            .next()
            .unwrap_or(rest)
    };
    // JSON/YAML null and empty containers carry no secret value. A labelled number does.
    !matches!(value, "null" | "~" | "{}" | "[]") && !placeholder(value)
}

pub fn contains_sensitive_text(text: &str) -> bool {
    let text = normalized(text);
    KNOWN.is_match(&text)
        || BASIC_AUTH.captures_iter(&text).any(|capture| {
            // RFC 7617 section 2 encodes user-id:password, not ordinary words
            // following "basic". Accept omitted padding for detection as well.
            // https://www.rfc-editor.org/rfc/rfc7617#section-2
            STANDARD
                .decode(&capture[1])
                .or_else(|_| STANDARD_NO_PAD.decode(&capture[1]))
                .is_ok_and(|bytes| bytes.contains(&b':'))
        })
        || ASSIGNMENT
            .find_iter(&text)
            .any(|m| has_value(&text[m.end()..]))
        || ENV_ASSIGNMENT
            .find_iter(&text)
            .any(|m| has_value(&text[m.end()..]))
        || PRIVATE_BLOCK
            .find_iter(&text)
            .any(|m| has_value(&text[m.end()..]))
        || ENV_BLOCK.find_iter(&text).any(|m| {
            let rest = &text[m.end()..];
            let value = rest.trim_start();
            let whitespace = &rest[..rest.len() - value.len()];
            (value.starts_with(['{', '[']) || whitespace.contains('\n')) && has_value(value)
        })
}

fn secret_key(key: &str) -> bool {
    let key = normalized(key);
    let lower = key.to_lowercase();
    let compact: String = lower
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-'))
        .collect();
    [
        "apikey",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "clientsecret",
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "authorization",
        "privateprompt",
        "密码",
        "密碼",
        "口令",
        "令牌",
        "密钥",
        "密鑰",
        "私有提示词",
        "私有提示詞",
        "私有prompt",
    ]
    .contains(&compact.as_str())
        || [
            "_api_key",
            "_access_token",
            "_refresh_token",
            "_client_secret",
            "_password",
            "_passwd",
            "_secret",
            "_token",
        ]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}
fn env_key(key: &str) -> bool {
    let key: String = normalized(key)
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '_' | '-'))
        .collect();
    [
        "env",
        "environment",
        "environmentvariables",
        "environmentvars",
        "environmentdump",
        "环境变量",
        "環境變數",
    ]
    .contains(&key.as_str())
}
fn has_json_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !placeholder(s),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        _ => true,
    }
}
pub fn contains_sensitive_value(value: &Value) -> bool {
    match value {
        Value::String(s) => contains_sensitive_text(s),
        Value::Array(values) => values.iter().any(contains_sensitive_value),
        Value::Object(values) => values.iter().any(|(key, value)| {
            contains_sensitive_text(key)
                || (secret_key(key) && has_json_value(value))
                || (env_key(key)
                    && (value.is_object() || value.is_array())
                    && has_json_value(value))
                || contains_sensitive_value(value)
        }),
        _ => false,
    }
}

pub fn ensure_public_text(text: &str) -> Result<()> {
    if contains_sensitive_text(text) {
        Err(Error::RuleViolation(REJECTION.into()))
    } else {
        Ok(())
    }
}
pub fn ensure_public_bytes(bytes: &[u8]) -> Result<()> {
    ensure_public_text(&String::from_utf8_lossy(bytes))?;
    // JSON reports can encode a private heading inside a string. Inspect decoded data too.
    if matches!(
        bytes.iter().find(|b| !b.is_ascii_whitespace()),
        Some(b'{' | b'[')
    ) && let Ok(value) = serde_json::from_slice::<Value>(bytes)
    {
        ensure_public_value(&value)?;
    }
    Ok(())
}
pub fn ensure_public_value(value: &Value) -> Result<()> {
    if contains_sensitive_value(value) {
        Err(Error::RuleViolation(REJECTION.into()))
    } else {
        Ok(())
    }
}
pub fn ensure_public_data<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    ensure_public_value(&serde_json::to_value(value)?)
}
pub fn safe_diagnostic(text: &str) -> String {
    if contains_sensitive_text(text) {
        SENSITIVE_CONTENT_WITHHELD.into()
    } else {
        text.into()
    }
}
pub fn redact_sensitive_value(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(safe_diagnostic(&s)),
        Value::Array(values) => {
            Value::Array(values.into_iter().map(redact_sensitive_value).collect())
        }
        Value::Object(values) => {
            if values.keys().any(|key| contains_sensitive_text(key)) {
                return Value::String(SENSITIVE_CONTENT_WITHHELD.into());
            }
            Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        let value = if (secret_key(&key) || env_key(&key)) && has_json_value(&value)
                        {
                            Value::String(SENSITIVE_CONTENT_WITHHELD.into())
                        } else {
                            redact_sensitive_value(value)
                        };
                        (key, value)
                    })
                    .collect(),
            )
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn labelled_values_and_environment_are_rejected_without_echo() {
        for text in [
            "password: fixture-value-only",
            "API_KEY=fixture-value-only",
            "access_token:\n  fixture-value-only",
            "authorization: fixture-value-only",
            "export HOME=/fixture/private",
            "private_prompt: fixture-value-only",
            "# 私有提示词\nfixture-value-only",
            "env: {HOME: /fixture}",
        ] {
            let error = ensure_public_text(text).unwrap_err();
            assert!(!error.report().message.contains("fixture"));
        }
        for value in [
            json!({"password":1234}),
            json!({"private_prompt":["fixture"]}),
            json!({"environment":{"HOME":"/fixture"}}),
        ] {
            assert!(ensure_public_value(&value).is_err());
        }
    }
    #[test]
    fn ordinary_discussion_and_explicit_placeholders_remain_usable() {
        ensure_public_text(&format!("task-{}", "a".repeat(40))).unwrap();
        for bytes in [
            br#"{"password":null}"#.as_slice(),
            br#"{"env":{}}"#,
            br#"{"env":[]}"#,
        ] {
            ensure_public_bytes(bytes).unwrap();
        }
        ensure_public_text("environment: candidate").unwrap();
        assert!(ensure_public_text("environment:\n  HOME: /fixture/private").is_err());
        for text in [
            "Review API key handling, token budgets, password protection and private prompts.",
            "讨论密码、令牌和环境变量的保护。",
            "password: [redacted]",
            "API_KEY=${EXAMPLE_API_KEY}",
            "token: \"\"",
            "password: null",
            "secret: ***",
        ] {
            ensure_public_text(text).unwrap();
        }
        ensure_public_value(&json!({"pending_secret_conditions":16,"token_budget":5000,"environment":"candidate","api_key":"${EXAMPLE_API_KEY}"})).unwrap();
        assert!(ensure_public_text("password: [redacted]fixture").is_err());
    }
    #[test]
    fn basic_prose_remains_usable() {
        for text in [
            "The basic source-intake ledger has no phase declaration.",
            "Review basic authentication and basic validation requirements.",
            "A basic\ncomponent requires an explicit work identity.",
            "The BASIC infrastructure documentation is ready.",
        ] {
            ensure_public_text(text).unwrap();
            ensure_public_value(&json!({"summary": text})).unwrap();
            assert_eq!(safe_diagnostic(text), text);
        }
    }
    #[test]
    fn basic_credentials_are_rejected_without_echo() {
        // Public synthetic user/password pairs, including short and unpadded values.
        for text in [
            "Basic dXNlcjpwYXNz",
            "basic dXNlcjo=",
            "Basic dXNlcjo",
            "Basic YTpi",
            "Basic Og==",
            "Ｂａｓｉｃ　dXNlcjpwYXNz",
        ] {
            assert!(ensure_public_text(text).is_err());
            assert!(ensure_public_value(&json!({"body": text})).is_err());
            assert_eq!(safe_diagnostic(text), SENSITIVE_CONTENT_WITHHELD);
        }
    }
    #[test]
    fn unicode_and_escaped_labels_cannot_bypass_detection() {
        for text in [
            "ＡＰＩ＿ＫＥＹ：fixture-value-only",
            "pass\u{200b}word: fixture-value-only",
            "密碼：fixture-value-only",
            r#"{"pa\u0073sword":"fixture-value-only"}"#,
        ] {
            assert!(ensure_public_text(text).is_err());
        }
        assert!(ensure_public_value(&json!({"ｐａｓｓｗｏｒｄ":"fixture-value-only"})).is_err());
    }
    #[test]
    fn recognizable_tokens_and_binary_content_are_rejected() {
        for prefix in ["sk-", "ghp_", "github_pat_", "xoxb-"] {
            let text = format!("{prefix}{}", "a".repeat(40));
            assert!(ensure_public_text(&text).is_err());
            let mut bytes = vec![0xff, 0, 1];
            bytes.extend(text.bytes());
            assert!(ensure_public_bytes(&bytes).is_err());
        }
        assert!(ensure_public_text("-----BEGIN PRIVATE KEY-----").is_err());
        assert!(ensure_public_text("https://fixture-user:fixture-pass@example.invalid").is_err());
        let report =
            serde_json::to_vec(&json!({"body":"# Private prompt\nfixture-value-only"})).unwrap();
        assert!(ensure_public_bytes(&report).is_err());
    }
    #[test]
    fn diagnostics_keep_receipt_identity_but_withhold_sensitive_reason() {
        let value = redact_sensitive_value(
            json!({"proposal_id":"fixture-id","reason":"password=fixture-value-only"}),
        );
        assert_eq!(value["proposal_id"], "fixture-id");
        assert!(!value.to_string().contains("fixture-value-only"));
    }
}
