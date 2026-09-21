use crate::error::{TeamError, TeamResult};
use crate::{PROTOCOL, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SURFACES: [&str; 3] = ["http", "mcp", "cli"];

/// Operations with a validated wire schema. This is NOT a dispatch registry:
/// only local capabilities are executable; claim is validation-only for now.
#[derive(Clone, Copy)]
enum Operation {
    Capabilities,
    Claim,
}
impl Operation {
    fn parse(name: &str) -> TeamResult<Self> {
        match name {
            "capabilities" => Ok(Self::Capabilities),
            "work.claim" => Ok(Self::Claim),
            _ => Err(TeamError::Unsupported),
        }
    }
    fn writes(self) -> bool {
        matches!(self, Self::Claim)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthContext {
    pub tenant_id: String,
    pub project_id: String,
    pub actor_id: String,
    pub client_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteProfile {
    pub name: String,
    pub endpoint: String,
    pub project_key: String,
    pub credential_env: String,
    pub protocol_version: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    protocol_version: u32,
    request_id: String,
    op: String,
    raw: Value,
}
impl Envelope {
    pub fn operation(&self) -> &str {
        &self.op
    }
}

impl RemoteProfile {
    pub fn validate(&self) -> TeamResult<()> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(TeamError::ProtocolUnsupported);
        }
        if self.project_key.trim().is_empty() {
            return Err(TeamError::ProjectRequired);
        }
        if !valid_env_ref(&self.credential_env) {
            return Err(TeamError::SecretRefInvalid);
        }
        validate_endpoint(&self.endpoint)?;
        Ok(())
    }

    pub fn redacted(&self) -> Value {
        json!({
            "name": self.name,
            "endpoint": safe_endpoint(&self.endpoint),
            "project_key": self.project_key,
            "credential_env": self.credential_env,
            "protocol_version": self.protocol_version,
        })
    }
}

pub fn parse_envelope(raw: &Value) -> TeamResult<Envelope> {
    if !raw.is_object() {
        return Err(TeamError::InvalidInput("envelope must be an object".into()));
    }
    let version = match raw.get("protocol_version") {
        Some(Value::Number(n)) => n.as_u64(),
        Some(Value::String(s)) => crate::decode_u64(s).ok(),
        _ => None,
    }
    .ok_or(TeamError::ProtocolUnsupported)?;
    let protocol_version = u32::try_from(version).map_err(|_| TeamError::ProtocolUnsupported)?;
    if protocol_version != PROTOCOL_VERSION {
        return Err(TeamError::ProtocolUnsupported);
    }
    let request_id = raw
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(TeamError::MissingRequiredField("request_id".into()))?
        .to_owned();
    let op = raw
        .get("op")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or(TeamError::MissingRequiredField("op".into()))?
        .to_owned();
    let operation = Operation::parse(&op)?;
    let empty = json!({});
    let args = raw
        .get("args")
        .unwrap_or(&empty)
        .as_object()
        .ok_or_else(|| TeamError::InvalidInput("args must be an object".into()))?;
    let allowed: &[&str] = match operation {
        Operation::Capabilities => &[],
        Operation::Claim => &[
            "work_id",
            "scope_id",
            "session_id",
            "expected_work_version",
            "expected_contract_hash",
        ],
    };
    for key in args.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(TeamError::UnknownRequiredField(key.clone()));
        }
    }
    if matches!(operation, Operation::Claim) {
        for key in [
            "work_id",
            "scope_id",
            "session_id",
            "expected_contract_hash",
        ] {
            if !args
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
            {
                return Err(TeamError::InvalidInput(format!(
                    "{key} must be a nonempty string"
                )));
            }
        }
        let version = args.get("expected_work_version");
        let valid = match version {
            Some(Value::String(s)) => crate::decode_u64(s).is_ok(),
            Some(Value::Number(n)) => n.as_u64().is_some(),
            _ => false,
        };
        if !valid {
            return Err(TeamError::InvalidInput(
                "expected_work_version must be an unsigned integer or canonical decimal string"
                    .into(),
            ));
        }
    }
    // Retain declarations so every shared entry checks them against an independent context.
    Ok(Envelope {
        protocol_version,
        request_id,
        op,
        raw: raw.clone(),
    })
}

pub fn authorize(auth: &AuthContext, body: &Value) -> TeamResult<()> {
    if auth.project_id.trim().is_empty() {
        return Err(TeamError::ProjectRequired);
    }
    if [&auth.tenant_id, &auth.actor_id, &auth.client_id]
        .iter()
        .any(|s| s.trim().is_empty())
    {
        return Err(TeamError::AuthProjectMismatch);
    }
    if !body.is_object() {
        return Err(TeamError::InvalidInput("envelope must be an object".into()));
    }
    for value in [Some(body), body.get("args")].into_iter().flatten() {
        for (key, expected) in [
            ("tenant_id", &auth.tenant_id),
            ("project_id", &auth.project_id),
            ("actor_id", &auth.actor_id),
            ("client_id", &auth.client_id),
        ] {
            if let Some(actual) = value.get(key) {
                if actual.as_str() != Some(expected.as_str()) {
                    return Err(TeamError::AuthProjectMismatch);
                }
            }
        }
    }
    Ok(())
}

/// Pure validation against an independently supplied context. This does not
/// authenticate a caller, submit a command, read a cache or create a receipt.
pub fn validate_only(surface: &str, envelope: &Envelope, auth: &AuthContext) -> TeamResult<Value> {
    if !SURFACES.contains(&surface) || envelope.protocol_version != PROTOCOL_VERSION {
        return Err(TeamError::ProtocolUnsupported);
    }
    authorize(auth, &envelope.raw)?;
    Ok(
        json!({"validation_only":true,"submitted":false,"authenticated":false,
        "surface":surface,"op":envelope.op,"request_id":envelope.request_id,
        "context":{"tenant_id":auth.tenant_id,"project_id":auth.project_id,"actor_id":auth.actor_id,"client_id":auth.client_id}}),
    )
}

pub fn execute(
    surface: &str,
    envelope: Envelope,
    auth: &AuthContext,
    remote: Option<&RemoteProfile>,
    online: bool,
) -> TeamResult<Value> {
    if !SURFACES.contains(&surface) {
        return Err(TeamError::ProtocolUnsupported);
    }
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err(TeamError::ProtocolUnsupported);
    }
    let write = Operation::parse(&envelope.op)?.writes();
    match remote {
        None => {
            if write || envelope.op != "capabilities" {
                return Err(TeamError::OfflineWriteForbidden);
            }
        }
        Some(profile) => profile.validate()?,
    }
    if write && !online {
        return Err(TeamError::OfflineWriteForbidden);
    }
    if envelope.op == "capabilities" {
        if !auth.project_id.is_empty() {
            authorize(auth, &envelope.raw)?;
        } else if ["tenant_id", "project_id", "actor_id", "client_id"]
            .iter()
            .any(|k| envelope.raw.get(k).is_some())
        {
            return Err(TeamError::AuthProjectMismatch);
        }
        return Ok(json!({
            "local": true,
            "submitted": false,
            "command_transport": false,
            "protocol": PROTOCOL,
            "protocol_version": PROTOCOL_VERSION,
            "authority_model": "approved-source-snapshot",
            "runtime_state_authority": "server",
            "offline_mutations": false,
            "arbitrary_external_exactly_once": false,
            "surface": surface,
            "project_id": auth.project_id,
        }));
    }
    validate_only(surface, &envelope, auth)?;
    Err(TeamError::Unsupported)
}

pub fn same_error_on_all_surfaces(
    envelope: Envelope,
    auth: &AuthContext,
    remote: Option<&RemoteProfile>,
    online: bool,
) -> TeamResult<()> {
    let first = execute("http", envelope.clone(), auth, remote, online);
    for surface in SURFACES {
        let next = execute(surface, envelope.clone(), auth, remote, online);
        match (&first, &next) {
            (Ok(a), Ok(b)) => {
                let mut a = a.clone();
                let mut b = b.clone();
                a.as_object_mut().map(|m| m.remove("surface"));
                b.as_object_mut().map(|m| m.remove("surface"));
                if a != b {
                    return Err(TeamError::InvalidContract("surfaces diverged".into()));
                }
            }
            (Err(a), Err(b)) if a == b => {}
            _ => {
                return Err(TeamError::InvalidContract("surfaces diverged".into()));
            }
        }
    }
    first.map(|_| ())
}

fn validate_endpoint(raw: &str) -> TeamResult<()> {
    let url = url::Url::parse(raw).map_err(|_| TeamError::SecretRefInvalid)?;
    let local = match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if url.host().is_none()
        || !(url.scheme() == "https" || (url.scheme() == "http" && local))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(TeamError::SecretRefInvalid);
    }
    Ok(())
}

// Independent output guard: even an unvalidated struct never echoes credentials.
fn safe_endpoint(raw: &str) -> String {
    if validate_endpoint(raw).is_err() {
        return "[invalid endpoint]".into();
    }
    raw.to_owned()
}

fn valid_env_ref(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some('A'..='Z') | Some('_') => {}
        _ => return false,
    }
    chars.all(|c| matches!(c, 'A'..='Z' | '0'..='9' | '_')) && !name.contains("SECRET_VALUE")
}
