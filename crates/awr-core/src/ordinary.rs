//! Ordinary confirmations are source-backed assertions, never engineering evidence.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryWorkPolicy {
    pub version: u32,
    pub policy_id: String,
    pub authorized_by: String,
    pub authorized_at: i64,
    pub reason: String,
    /// Exact external keys within this registered ledger. No title-based inference.
    pub work_items: Vec<String>,
}
impl OrdinaryWorkPolicy {
    pub fn validate(&self) -> Result<()> {
        ensure_public_data(self)?;
        validate_criteria(&self.work_items)?;
        if self.version != 1
            || self.work_items.len() > 1000
            || self.authorized_at <= 0
            || self.authorized_at > now_millis()? + 60_000
            || [&self.policy_id, &self.authorized_by, &self.reason]
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 4096)
        {
            return Err(Error::InvalidInput("ordinary policy needs version 1, explicit authorization, reason and exact work scope".into()));
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
    pub fn from_config(config: &serde_json::Value) -> Result<Option<Self>> {
        config["adapter_options"]
            .get("ordinary_work_policy")
            .map(|v| {
                let p: Self = serde_json::from_value(v.clone())?;
                p.validate()?;
                Ok(p)
            })
            .transpose()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdinaryCompletionKind {
    UserConfirmation,
    BusinessCheck,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryArtifact {
    pub locator: String,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrdinaryCompletion {
    pub version: u32,
    pub request_key: String,
    pub policy: OrdinaryWorkPolicy,
    pub policy_fingerprint: String,
    pub kind: OrdinaryCompletionKind,
    pub actor: HostActor,
    pub basis: String,
    pub confirmed_at: i64,
    pub acceptance: Vec<String>,
    pub artifacts: Vec<OrdinaryArtifact>,
}
impl OrdinaryCompletion {
    pub fn validate(&self, key: &str, acceptance: &[String]) -> Result<()> {
        ensure_public_data(self)?;
        self.actor.validate()?;
        validate_criteria(acceptance)?;
        if self.version != 1
            || self.policy_fingerprint != self.policy.fingerprint()?
            || !self.policy.work_items.iter().any(|w| w == key)
            || self.request_key.trim().is_empty()
            || self.request_key.len() > 512
            || self.basis.trim().is_empty()
            || self.basis.len() > 4096
            || self.confirmed_at <= 0
            || self.confirmed_at > now_millis()? + 60_000
            || self.confirmed_at < self.policy.authorized_at
            || self.acceptance != acceptance
            || self.artifacts.len() > 32
            || (self.kind == OrdinaryCompletionKind::UserConfirmation
                && self.actor.origin != HostEditOrigin::Human)
            || (self.kind == OrdinaryCompletionKind::BusinessCheck && self.artifacts.is_empty())
            || self.artifacts.iter().any(|a| {
                a.locator.trim().is_empty() || a.locator.len() > 4096 || !is_sha256_hash(&a.sha256)
            })
        {
            return Err(Error::RuleViolation("ordinary completion requires the scoped policy, actual actor, current criteria, basis, time and required artifacts".into()));
        }
        Ok(())
    }
}
