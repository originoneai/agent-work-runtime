use crate::{EntityKind, Error, Id, MutationProposal, ProjectionMeta, ProposalStatus, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// A single source projection, including its stable identity and exact source pointer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationTarget {
    pub kind: EntityKind,
    pub meta: ProjectionMeta,
}

/// Versioned envelope stored in mutation_proposals.patch_json. Changes are proposed field
/// replacements, not SQL, a whole-file replacement, or authorization to bypass domain checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationPatch {
    pub version: u32,
    pub target: MutationTarget,
    pub source_config: Value,
    pub intent: String,
    pub changes: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_action: Option<crate::WorkActionBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_edit: Option<HostEditBinding>,
}

/// Caller-supplied provenance, never authentication or permission to bypass a domain gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostEditBinding {
    pub version: u32,
    pub request_key: String,
    pub request_hash: String,
    pub actor: HostActor,
    pub action: HostEditAction,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostActor {
    pub host: String,
    pub subject: String,
    pub origin: HostEditOrigin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostEditOrigin {
    Human,
    AiAccepted,
    DelegatedAgent,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostEditAction {
    Fields,
    ActivateDraft,
    ConfirmOrdinary,
}
impl HostActor {
    pub fn validate(&self) -> Result<()> {
        crate::ensure_public_data(self)?;
        if self.host.len() + self.subject.len() + 1 > 256 {
            return Err(Error::InvalidInput(
                "combined host and subject must fit the 256-byte actor receipt limit".into(),
            ));
        }
        for value in [&self.host, &self.subject] {
            if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
                return Err(Error::InvalidInput(
                    "host and subject require bounded provenance identifiers".into(),
                ));
            }
        }
        Ok(())
    }
}
impl HostEditBinding {
    pub fn validate(&self) -> Result<()> {
        self.actor.validate()?;
        if self.version != 1
            || self.request_key.trim().is_empty()
            || self.request_key.len() > 512
            || self.request_key.chars().any(char::is_control)
            || !crate::is_sha256_hash(&self.request_hash)
        {
            return Err(Error::InvalidInput(
                "host edit requires version, bounded request key and exact request hash".into(),
            ));
        }
        Ok(())
    }
}
impl MutationPatch {
    pub fn mutation_type(&self) -> &'static str {
        self.work_action
            .as_ref()
            .map_or("update_fields", |binding| binding.action.mutation_type())
    }
    pub fn validate(&self) -> Result<()> {
        crate::ensure_public_data(self)?;
        if self.version != 1 || self.intent.trim().is_empty() || self.intent.len() > 4096 {
            return Err(Error::InvalidInput(
                "proposal requires patch version 1 and an intent of 1..4096 bytes".into(),
            ));
        }
        let fields = self
            .changes
            .as_object()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                Error::InvalidInput(
                    "proposal changes must be a nonempty object of field replacements".into(),
                )
            })?;
        if fields.keys().any(|k| {
            k.trim().is_empty()
                || matches!(
                    k.as_str(),
                    "id" | "external_key"
                        | "project_id"
                        | "source_id"
                        | "source_ref"
                        | "revision"
                        | "meta"
                )
        }) {
            return Err(Error::InvalidInput(
                "proposal cannot change identity or source-binding fields".into(),
            ));
        }
        if serde_json::to_vec(self)?.len() > 64 * 1024 {
            return Err(Error::InvalidInput("proposal patch exceeds 64 KiB".into()));
        }
        if let Some(binding) = &self.work_action {
            if self.target.kind != EntityKind::WorkItem {
                return Err(Error::InvalidInput(
                    "work actions require an exact WorkItem target".into(),
                ));
            }
            binding.validate(&self.changes)?;
        }
        if let Some(binding) = &self.host_edit {
            binding.validate()?;
            if self.work_action.is_some() {
                return Err(Error::InvalidInput(
                    "host edits cannot impersonate Agent work actions".into(),
                ));
            }
            if binding.action == HostEditAction::ActivateDraft
                && (self.target.kind != EntityKind::WorkItem
                    || self.changes != serde_json::json!({"status":"planned"}))
            {
                return Err(Error::RuleViolation(
                    "draft activation only declares the planned state of one work item".into(),
                ));
            }
            if binding.action == HostEditAction::ConfirmOrdinary {
                let receipt: crate::OrdinaryCompletion =
                    serde_json::from_value(self.changes["ordinary_completion"].clone())?;
                receipt.validate(&self.target.meta.external_key, &receipt.acceptance)?;
                if self.target.kind != EntityKind::WorkItem
                    || fields.len() != 2
                    || self.changes["status"] != "completed"
                    || receipt.actor != binding.actor
                    || receipt.request_key != binding.request_key
                    || crate::OrdinaryWorkPolicy::from_config(&self.source_config)?.as_ref()
                        != Some(&receipt.policy)
                {
                    return Err(Error::RuleViolation("ordinary confirmation cannot change its actor, policy, scope or engineering metadata".into()));
                }
            }
        }
        let meta = &self.target.meta;
        let source = &meta.source_ref;
        if meta.external_key.trim().is_empty()
            || meta.revision == 0
            || source.source_revision == 0
            || source.locator.is_empty()
            || !self.source_config.is_object()
            || !source
                .source_fingerprint
                .strip_prefix("sha256:")
                .is_some_and(crate::is_sha256_hash)
            || source.pointer.as_ref().is_none_or(|p| p.trim().is_empty())
        {
            return Err(Error::InvalidInput("proposal requires an exact indexed target, pointer, source fingerprint and configuration".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationDraft {
    pub source_id: Id,
    pub base_fingerprint: String,
    pub mutation_type: String,
    pub patch: MutationPatch,
    pub created_by_session: Option<Id>,
}

/// No public action can assert Applied. That state requires a verified source-write receipt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalAction {
    Submit,
    Approve,
    Reject,
    Conflict,
    Fail,
    RequireManualApply,
}
impl ProposalAction {
    pub fn next_status(self, current: ProposalStatus) -> Result<ProposalStatus> {
        use ProposalStatus::*;
        let open = matches!(current, Draft | Ready | Approved);
        match (self, current) {
            (Self::Submit, Draft) => Ok(Ready),
            (Self::Approve, Ready) => Ok(Approved),
            (Self::Reject, _) if open => Ok(Rejected),
            (Self::Conflict, _) if open => Ok(Conflict),
            (Self::Fail, _) if open => Ok(Failed),
            (Self::RequireManualApply, Approved) => Ok(Approved),
            _ => Err(Error::InvalidTransition(format!(
                "cannot {self:?} a {current:?} proposal"
            ))),
        }
    }
    pub fn event_type(self) -> &'static str {
        match self {
            Self::Submit => "proposal.ready",
            Self::Approve => "proposal.approved",
            Self::Reject => "proposal.rejected",
            Self::Conflict => "proposal.conflict",
            Self::Fail => "proposal.failed",
            Self::RequireManualApply => "proposal.required",
        }
    }
}
impl ProposalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Approved => "approved",
            Self::Applied => "applied",
            Self::Conflict => "conflict",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}
impl MutationProposal {
    pub fn bound_patch(&self) -> Result<MutationPatch> {
        let patch: MutationPatch = serde_json::from_value(self.patch.clone()).map_err(|e| {
            Error::InvalidInput(format!("proposal lacks a supported source binding: {e}"))
        })?;
        patch.validate()?;
        if self.mutation_type != patch.mutation_type()
            || patch.target.meta.source_ref.source_id != self.source_id
            || patch.target.meta.source_ref.source_fingerprint != self.base_fingerprint
        {
            return Err(Error::InvalidInput(
                "proposal envelope disagrees with its immutable binding".into(),
            ));
        }
        Ok(patch)
    }
}

/// Projection identity and facts, excluding the metadata refreshed by a source reindex.
pub fn mutation_projection_hash(value: &Value) -> Result<String> {
    let mut facts = value.clone();
    let fields = facts
        .as_object_mut()
        .ok_or_else(|| Error::InvalidInput("mutation target must be an object".into()))?;
    fields.remove("source_ref");
    fields.remove("revision");
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&facts)?)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MutationWritePlan {
    pub id: Id,
    pub before_fingerprint: String,
    pub after_fingerprint: String,
    pub before_size: u64,
    pub after_size: u64,
    pub target_after_hash: String,
}
impl MutationWritePlan {
    pub fn recovery_directory(&self) -> String {
        format!(".awr/mutations/{}", self.id)
    }
    pub fn validate(&self) -> Result<()> {
        if ![&self.before_fingerprint, &self.after_fingerprint]
            .iter()
            .all(|fp| {
                fp.strip_prefix("sha256:")
                    .is_some_and(crate::is_sha256_hash)
            })
            || self.before_fingerprint == self.after_fingerprint
            || !crate::is_sha256_hash(&self.target_after_hash)
            || self.before_size == 0
            || self.after_size == 0
            || self.before_size > 16 * 1024 * 1024
            || self.after_size > 16 * 1024 * 1024
        {
            return Err(Error::InvalidInput(
                "invalid mutation write plan fingerprints, target hash or bounded sizes".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationApplyAttempt {
    pub event_id: Id,
    pub proposal_id: Id,
    pub source_id: Id,
    pub project_revision: crate::Revision,
    pub plan: MutationWritePlan,
    pub resolved_event_id: Option<Id>,
}
