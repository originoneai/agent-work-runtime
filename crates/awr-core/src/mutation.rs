use crate::{EntityKind, Error, Id, MutationProposal, ProjectionMeta, ProposalStatus, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}
impl MutationPatch {
    pub fn validate(&self) -> Result<()> {
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
        if self.mutation_type != "update_fields"
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
