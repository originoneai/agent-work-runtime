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

pub const SECRET_POLICY_VERSION: u32 = 4;
pub const SENSITIVE_CONTENT_WITHHELD: &str = "[sensitive content withheld]";
const REJECTION: &str =
    "sensitive content is not accepted; remove secret values or use explicit redacted placeholders";

static KNOWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)(?:^|[^\p{L}\p{N}_])(?:sk-[a-z0-9_-]{16,}|gh[pousr]_[a-z0-9]{20,}|github_pat_[a-z0-9_]{20,}|",
        r"xox[baprs]-[a-z0-9-]{12,}|\b(?:AKIA|ASIA)[A-Z0-9]{16}\b|",
        r"\beyJ[a-z0-9_-]{8,}\.[a-z0-9_-]{8,}\.[a-z0-9_-]{8,}|",
        r"-----BEGIN (?:[A-Z0-9]+ )?PRIVATE KEY-----|",
        r"[a-z][a-z0-9+.-]*://[^\s/@:]+:[^\s/@]+@)"
    ))
    .expect("fixed credential pattern")
});

static BEARER_AUTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^\p{L}\p{N}_])bearer[\s]+([a-z0-9+/_=-]+)")
        .expect("fixed Bearer pattern")
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
    r#"[\s\"'`]*[:=]"#
)).expect("fixed secret assignment pattern")
});

static ENV_ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[\s;])(?:export[ \t]+)?[A-Z_][A-Z0-9_]{1,80}[ \t]*=[ \t]*")
        .expect("fixed environment assignment pattern")
});

/// A diagnostic category is safe to disclose; matched text and key names are not.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveCategory {
    Credential,
    LabelledValue,
    EnvironmentDump,
    PrivatePrompt,
}
impl SensitiveCategory {
    fn rejection(self) -> String {
        format!("{REJECTION}; category={}", self.name())
    }
    fn name(self) -> &'static str {
        match self {
            Self::Credential => "credential",
            Self::LabelledValue => "labelled_value",
            Self::EnvironmentDump => "environment_dump",
            Self::PrivatePrompt => "private_prompt",
        }
    }
}

pub(crate) fn sensitive_rejection_details(message: &str) -> Option<Value> {
    [SensitiveCategory::Credential, SensitiveCategory::LabelledValue,
        SensitiveCategory::EnvironmentDump, SensitiveCategory::PrivatePrompt]
        .into_iter().find(|category| message == category.rejection()).map(|category| {
            serde_json::json!({"policy_version": SECRET_POLICY_VERSION, "category": category,
                "next_action": "Inspect this source locally. Keep credentials outside AWR; use explicit redacted placeholders for values. Describe public configuration and permission outcomes as prose, not a credential field or environment dump."})
        })
}

// Environment dumps and explicit exports retain their boundary. An incidental
// assignment in a command/prose string is not an environment dump. Sensitive key
// labels and recognizable credentials are still checked independently everywhere.
fn environment_assignment(text: &str, matched: &str, start: usize) -> bool {
    if matched.trim_start().starts_with("export") {
        return true;
    }
    let line_start = text[..start].rfind('\n').map_or(0, |n| n + 1);
    let prefix = text[line_start..start].trim();
    let line = text[line_start..].lines().next().unwrap_or("").trim();
    (prefix.is_empty() || prefix.chars().all(|c| matches!(c, '`' | '\'' | '"')))
        && !inline_command(line)
}
fn inline_command(line: &str) -> bool {
    let mut assignment = false;
    for word in line.split_whitespace() {
        if let Some((name, value)) = word.split_once('=') {
            if !env_name(name)
                || name != name.to_ascii_uppercase()
                || value.is_empty()
                || value.contains(['\'', '"', '`', ';'])
            {
                return false;
            }
            assignment = true;
        } else {
            return assignment
                && word.chars().any(char::is_alphabetic)
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c));
        }
    }
    false
}

// "completed native authorization: the user accepted ..." is a narrative event,
// unlike a line/header/JSON/YAML field named authorization. Only the former may
// carry prose. A single opaque word or an authentication scheme remains a value.
fn narrative_authorization(text: &str, matched: &str, start: usize, end: usize) -> bool {
    if matched.trim() != "authorization:" {
        return false;
    }
    let prefix = text[..start].rsplit('\n').next().unwrap_or("").trim();
    if !prefix.chars().last().is_some_and(char::is_alphabetic) {
        return false;
    }
    let rest = text[end..].trim_start().lines().next().unwrap_or("");
    let first = rest.split_whitespace().next().unwrap_or("");
    if first.to_ascii_lowercase().starts_with("bearer")
        || first.to_ascii_lowercase().starts_with("basic")
    {
        return false;
    }
    if first.is_ascii() {
        first.chars().all(char::is_alphabetic)
            && first.len() < 24
            && rest.split_whitespace().take(3).count() == 3
    } else {
        // A CJK sentence has explicit prose punctuation, unlike an opaque value.
        rest.chars().any(|c| matches!(c, '，' | '。' | '；' | '：'))
            && first
                .chars()
                .next()
                .is_some_and(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
    }
}

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

pub fn sensitive_text_category(text: &str) -> Option<SensitiveCategory> {
    let text = normalized(text);
    if KNOWN.is_match(&text)
        || BEARER_AUTH.captures_iter(&text).any(|capture| {
            let value = &capture[1];
            // An unlabelled ordinary word is prose, not proof of an opaque token.
            // Actual Authorization assignments/headers below reject values of ANY shape/length.
            value.len() >= 32
                || (value.len() >= 8
                    && (value
                        .bytes()
                        .any(|b| b.is_ascii_digit() || b"+/_=-".contains(&b))
                        || (value.bytes().skip(1).any(|b| b.is_ascii_uppercase())
                            && value.bytes().any(|b| b.is_ascii_lowercase()))))
        })
        || BASIC_AUTH.captures_iter(&text).any(|capture| {
            // RFC 7617 section 2 encodes user-id:password, not ordinary words
            // following "basic". Accept omitted padding for detection as well.
            // https://www.rfc-editor.org/rfc/rfc7617#section-2
            STANDARD
                .decode(&capture[1])
                .or_else(|_| STANDARD_NO_PAD.decode(&capture[1]))
                .is_ok_and(|bytes| bytes.contains(&b':'))
        })
    {
        return Some(SensitiveCategory::Credential);
    }
    if ASSIGNMENT.find_iter(&text).any(|m| {
        has_value(&text[m.end()..])
            && !definition_after_assignment(&text[m.end()..])
            && !narrative_authorization(&text, m.as_str(), m.start(), m.end())
    }) {
        return Some(SensitiveCategory::LabelledValue);
    }
    if ENV_ASSIGNMENT.find_iter(&text).any(|m| {
        environment_assignment(&text, m.as_str(), m.start())
            && has_value(&text[m.end()..])
            && !definition_after_assignment(&text[m.end()..])
    }) {
        return Some(SensitiveCategory::EnvironmentDump);
    }
    if PRIVATE_BLOCK
        .find_iter(&text)
        .any(|m| has_value(&text[m.end()..]))
    {
        return Some(SensitiveCategory::PrivatePrompt);
    }
    if ENV_BLOCK.find_iter(&text).any(|m| {
        let rest = &text[m.end()..];
        let value = rest.trim_start();
        let whitespace = &rest[..rest.len() - value.len()];
        (value.starts_with(['{', '[']) || whitespace.contains('\n'))
            && has_value(value)
            && !definition_after_assignment(rest)
    }) {
        return Some(SensitiveCategory::EnvironmentDump);
    }
    None
}
pub fn contains_sensitive_text(text: &str) -> bool {
    sensitive_text_category(text).is_some()
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

/// A definition carries structure, never a credential value. Extra fields are not ignored:
/// default/example/enum values must be absent, empty or explicitly redacted. This applies
/// equally to JSON Schema, YAML schema fragments and OpenAPI authentication schemes.
fn public_definition(value: &Value) -> bool {
    let Some(fields) = value.as_object() else {
        return false;
    };
    if !fields.contains_key("type") && !fields.contains_key("$ref") {
        return false;
    }
    fields.iter().all(|(key, value)| match key.as_str() {
        "type" => value.as_str().is_some_and(|s| {
            [
                "null",
                "boolean",
                "object",
                "array",
                "number",
                "string",
                "integer",
                "http",
                "apiKey",
                "oauth2",
                "openIdConnect",
            ]
            .contains(&s)
        }),
        "scheme" => value.as_str().is_some_and(|s| {
            ["bearer", "basic", "digest", "negotiate"].contains(&s.to_ascii_lowercase().as_str())
        }),
        "in" => value
            .as_str()
            .is_some_and(|s| ["header", "query", "cookie"].contains(&s)),
        "$ref" | "$schema" | "title" | "description" | "format" | "pattern" | "name"
        | "bearerFormat" | "openIdConnectUrl" => {
            value.as_str().is_some_and(|s| !contains_sensitive_text(s))
        }
        "properties" | "$defs" | "definitions" => value.as_object().is_some_and(|o| {
            o.iter()
                .all(|(k, v)| !contains_sensitive_text(k) && public_definition(v))
        }),
        "items" => public_definition(value),
        "additionalProperties" => value.is_boolean() || public_definition(value),
        "required" => value.as_array().is_some_and(|a| {
            a.iter()
                .all(|v| v.as_str().is_some_and(|s| !contains_sensitive_text(s)))
        }),
        "minLength" | "maxLength" | "minItems" | "maxItems" => value.as_u64().is_some(),
        "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" | "multipleOf" => {
            value.is_number()
        }
        "readOnly" | "writeOnly" | "deprecated" | "nullable" | "uniqueItems" => value.is_boolean(),
        "default" | "example" | "const" => !has_json_value(value),
        "examples" | "enum" => value
            .as_array()
            .is_some_and(|a| a.iter().all(|v| !has_json_value(v))),
        _ => false,
    })
}
fn definition_after_assignment(rest: &str) -> bool {
    const LIMIT: usize = 16 * 1024;
    let trimmed = rest.trim_start();
    if trimmed.starts_with('{') {
        let mut cap = trimmed.len().min(LIMIT);
        while !trimmed.is_char_boundary(cap) {
            cap -= 1;
        }
        let mut values = serde_json::Deserializer::from_str(&trimmed[..cap]).into_iter::<Value>();
        if let Some(Ok(value)) = values.next() {
            let end = values.byte_offset();
            let tail = &trimmed[end..];
            return end <= LIMIT
                && (tail.is_empty()
                    || tail.starts_with(|c: char| c.is_whitespace() || ",;}])`".contains(c)))
                && public_definition(&value)
                // JSON Value keeps the last duplicate key. A raw definition must not
                // hide an earlier default/example by replacing that key with null.
                && yaml_definition(&trimmed[..end]);
        }
        // YAML flow mappings use unquoted keys. Parse only the bounded line, without
        // consuming following prose or accepting trailing non-schema material.
        let line = trimmed.lines().next().unwrap_or("");
        if line.len() <= LIMIT {
            return yaml_definition(line);
        }
    } else if rest[..rest.len() - trimmed.len()].contains('\n') {
        let mut lines = rest.lines().skip_while(|line| line.trim().is_empty());
        if let Some(first) = lines.next() {
            let indent = first.len() - first.trim_start().len();
            if indent == 0 {
                return false;
            }
            let mut block = first.to_owned();
            for line in lines {
                if !line.trim().is_empty() && line.len() - line.trim_start().len() < indent {
                    break;
                }
                if block.len() + line.len() + 1 > LIMIT {
                    return false;
                }
                block.push('\n');
                block.push_str(line);
            }
            return block.len() <= LIMIT && yaml_definition(&block);
        }
    }
    false
}
fn yaml_definition(text: &str) -> bool {
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text)
        .ok()
        .and_then(|v| serde_json::to_value(v).ok())
        .is_some_and(|v| public_definition(&v))
}
fn sensitive_field(key: &str, value: &Value) -> bool {
    (secret_key(key) || (env_key(key) && (value.is_object() || value.is_array())))
        && has_json_value(value)
        && !public_definition(value)
}
pub fn contains_sensitive_value(value: &Value) -> bool {
    sensitive_value_category(value).is_some()
}
pub fn sensitive_value_category(value: &Value) -> Option<SensitiveCategory> {
    match value {
        Value::String(s) => sensitive_text_category(s),
        Value::Array(values) => values.iter().find_map(sensitive_value_category),
        Value::Object(values) => values.iter().find_map(|(key, value)| {
            sensitive_text_category(key).or_else(|| {
                if sensitive_field(key, value) {
                    Some(if env_key(key) {
                        SensitiveCategory::EnvironmentDump
                    } else {
                        SensitiveCategory::LabelledValue
                    })
                } else {
                    sensitive_value_category(value)
                }
            })
        }),
        _ => None,
    }
}

pub fn ensure_public_text(text: &str) -> Result<()> {
    if let Some(category) = sensitive_text_category(text) {
        Err(Error::RuleViolation(category.rejection()))
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
    if let Some(category) = sensitive_value_category(value) {
        Err(Error::RuleViolation(category.rejection()))
    } else {
        Ok(())
    }
}
pub fn ensure_public_data<T: Serialize + ?Sized>(value: &T) -> Result<()> {
    ensure_public_value(&serde_json::to_value(value)?)
}
/// Validate the complete source before shortening optional display text. Whitespace
/// folding/truncation can split an otherwise public schema; back up to a word boundary
/// in that case. Callers retain a source reference for the complete original content.
pub fn public_summary(text: &str, limit: usize) -> Result<String> {
    ensure_public_text(text)?;
    if limit == 0 {
        return Ok(String::new());
    }
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= limit && !contains_sensitive_text(&clean) {
        return Ok(clean);
    }
    let mut prefix: String = clean.chars().take(limit.saturating_sub(1)).collect();
    loop {
        let shortened = format!("{}…", prefix.trim_end());
        if !contains_sensitive_text(&shortened) {
            return Ok(shortened);
        }
        match prefix.rfind(char::is_whitespace) {
            Some(end) => prefix.truncate(end),
            None => prefix.clear(),
        }
    }
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
                        let value = if sensitive_field(&key, &value) {
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
    fn command_configuration_and_permission_narratives_are_not_dumps_or_headers() {
        for text in [
            "Expected count APP_EXPECTED_TASKS=134; keep the original source.",
            "# APP_EXPECTED_TASKS=134",
            "PROJECT_ROOT=/public/project EXPECTED_ITEMS=134 cargo test --offline",
            "TASK_LIMIT=25 ./check",
            "passed normal native authorization: 官方设备确认后应用继续工作；重开保留原记录。",
            "Completed native authorization: the user confirmed this operation.",
        ] {
            ensure_public_text(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            ensure_public_value(&json!({"summary":text})).unwrap();
            ensure_public_bytes(&serde_json::to_vec(&json!({"summary":text})).unwrap()).unwrap();
            ensure_public_text(&public_summary(text, 240).unwrap()).unwrap();
        }
        for text in [
            "APP_VALUE=unknown-value",
            "HOME=/private PATH=/private/bin",
            "Run export HOME=/private",
            "API_KEY=unknown-value cargo test",
            "Run API_KEY=unknown-value cargo test",
            "Authorization: the user confirmed this operation.",
            "native authorization: unknown-value",
            "native authorization: Bearer word",
            "native authorization: Basic word",
            "native authorization: aBcdEfgHijKlmNopQrStUvWxyz extra words",
            "authorization: 用户明确允许本轮编辑",
        ] {
            assert!(ensure_public_text(text).is_err(), "{text}");
        }
        assert!(
            ensure_public_value(&json!({"authorization":"the user confirmed this operation"}))
                .is_err()
        );
    }

    #[test]
    fn rejection_categories_preserve_the_legacy_code_without_disclosing_values() {
        for (text, category) in [
            ("export HOME=/private", "environment_dump"),
            ("password: synthetic-private-value", "labelled_value"),
            ("Basic YTpi", "credential"),
            ("# Private prompt\nprivate material", "private_prompt"),
        ] {
            let report = ensure_public_text(text).unwrap_err().report();
            assert_eq!(report.code, "RuleViolation");
            assert_eq!(report.details.as_ref().unwrap()["category"], category);
            assert_eq!(report.details.as_ref().unwrap()["policy_version"], 4);
            assert!(!serde_json::to_string(&report).unwrap().contains(text));
        }
    }

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
    fn public_schema_and_authentication_definitions_are_not_values() {
        for text in [
            "Bearer authentication and bearer authorization are protocol concepts.",
            "Discuss Bearer credentials and Bearer authentication requirements.",
            r#"{"token":{"type":"string","pattern":"^[a-z][a-z0-9_.-]{1,63}$"}}"#,
            r#"{"authorization":{"type":"http","scheme":"bearer"}}"#,
            "# Schema\n```json\n{\"password\":{\"type\":\"string\",\"format\":\"password\",\"description\":\"Supplied at runtime\"}}\n```\n",
            "# Schema\n```yaml\ntoken:\n  type: string\n  description: Supplied at runtime\n```\n",
            "password: {type: string, format: password}\n",
            "TOKEN={\"type\":\"string\"}\n",
        ] {
            ensure_public_text(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            ensure_public_bytes(text.as_bytes()).unwrap();
            ensure_public_value(&json!({"body":text})).unwrap();
            assert_eq!(safe_diagnostic(text), text);
        }
        for value in [
            json!({"token":{"type":"string"}}),
            json!({"type":"object","properties":{"password":{"type":"string","format":"password"}},"required":["password"]}),
            json!({"authorization":{"type":"http","scheme":"bearer"}}),
            json!({"token":{"type":"string","default":"[redacted]"}}),
        ] {
            ensure_public_value(&value).unwrap();
            assert_eq!(redact_sensitive_value(value.clone()), value);
        }
        let text = format!(
            "{}Schema: {{\"token\":{{\"type\":\"string\"}}}}",
            "Public discussion. ".repeat(12)
        );
        let short = public_summary(&text, 240).unwrap();
        assert!(short.chars().count() <= 240);
        ensure_public_text(&short).unwrap();
        assert!(
            public_summary(
                &format!("{}password: fixture-value", "Public. ".repeat(80)),
                240
            )
            .is_err()
        );
    }

    #[test]
    fn definitions_cannot_hide_credentials_or_data_values() {
        for value in [
            json!({"token":{"type":"string","default":"fixture-value"}}),
            json!({"token":{"type":"string","example":"fixture-value"}}),
            json!({"token":{"type":"string","enum":["fixture-value"]}}),
            json!({"authorization":{"type":"http","scheme":"bearer","value":"fixture-value"}}),
            json!({"token":{"type":"object","properties":{"client_secret":{"type":"string","default":"fixture-value"}}}}),
            json!({"token":{"type":"string"},"password":"fixture-value"}),
            json!({"token":"{\"type\":\"string\"}"}),
        ] {
            assert!(ensure_public_value(&value).is_err(), "{value}");
            assert!(
                ensure_public_bytes(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{value}"
            );
            assert!(
                !redact_sensitive_value(value)
                    .to_string()
                    .contains("fixture-value")
            );
        }
        for text in [
            "Authorization: Bearer word",
            "Authorization: Bearer authentication",
            "Bearer fixture-token-1234",
            "Bearer aBcdEfgHijKlmNopQrStUvWxyz",
            "token: {type: string, default: fixture-value}\n",
            "token:\n  type: string\n  default: fixture-value\n",
            "token: {\"type\":\"string\"}fixture-value",
            "token: {\"type\":\"string\"}\npassword: fixture-value",
            "token: {\"type\":\"string\",\"default\":\"fixture-value\",\"default\":null}",
            "token:\n  type: string\n  default: fixture-value\n  default: null\n",
        ] {
            assert!(ensure_public_text(text).is_err(), "{text}");
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
