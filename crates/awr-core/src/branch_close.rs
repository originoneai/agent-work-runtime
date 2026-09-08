use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchCloseOutcome {
    Merged,
    Abandoned,
}
impl BranchCloseOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merged => "merged",
            Self::Abandoned => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BranchMergeInput {
    Git {
        source_ref: String,
        target_ref: String,
    },
    /// A caller-provided source merge report. Runtime verifies the file/hash, not its prose.
    Source { locator: String, sha256: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BranchMergeObservation {
    Git {
        source: GitRefBinding,
        target: GitRefBinding,
        head_sha: String,
    },
    Source {
        locator: String,
        sha256: String,
        size: u64,
        observed_at: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum BranchLoopResolution {
    Resolved {
        reason: String,
        references: Vec<String>,
    },
    CarryForward {
        reason: String,
        work_item_key: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchLoopDisposition {
    pub checkpoint_id: Id,
    pub index: usize,
    pub text: String,
    pub resolution: BranchLoopResolution,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloseBranchInput {
    pub version: u32,
    pub outcome: BranchCloseOutcome,
    pub summary: String,
    pub merge: Option<BranchMergeInput>,
    pub open_loops: Vec<BranchLoopDisposition>,
}
impl CloseBranchInput {
    pub fn validate(&self) -> Result<()> {
        let nonblank = |s: &str| !s.trim().is_empty() && s.len() <= 65536;
        if self.version != 1
            || !nonblank(&self.summary)
            || self.open_loops.len() > 1000
            || (self.outcome == BranchCloseOutcome::Merged) != self.merge.is_some()
        {
            return Err(Error::InvalidInput("branch close requires version 1, a summary, at most 1000 loop dispositions and merge evidence exactly when outcome is merged".into()));
        }
        if let Some(merge) = &self.merge {
            match merge {
                BranchMergeInput::Git {
                    source_ref,
                    target_ref,
                } => {
                    validate_git_ref(source_ref)?;
                    validate_git_ref(target_ref)?;
                }
                BranchMergeInput::Source { locator, sha256 } => {
                    if !nonblank(locator) || !is_sha256_hash(sha256) {
                        return Err(Error::InvalidInput(
                            "source merge needs a report locator and SHA256".into(),
                        ));
                    }
                }
            }
        }
        for item in &self.open_loops {
            let valid = nonblank(&item.text)
                && match &item.resolution {
                    BranchLoopResolution::Resolved { reason, references } => {
                        nonblank(reason)
                            && !references.is_empty()
                            && references.len() <= 100
                            && references.iter().all(|r| nonblank(r))
                    }
                    BranchLoopResolution::CarryForward {
                        reason,
                        work_item_key,
                    } => nonblank(reason) && nonblank(work_item_key),
                };
            if !valid {
                return Err(Error::InvalidInput("each open loop needs its exact text and a reason with resolution references or a target work key".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchOpenLoop {
    pub checkpoint_id: Id,
    pub checkpoint_revision: Revision,
    pub session_id: Id,
    pub work_item_id: Option<Id>,
    pub index: usize,
    pub text: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BranchClosePlan {
    pub project_id: Id,
    pub project_revision: Revision,
    pub current_branch_id: Option<Id>,
    pub branch: Branch,
    pub sessions: Vec<Session>,
    pub unsettled_claims: Vec<Claim>,
    pub open_loops: Vec<BranchOpenLoop>,
    pub pending_checkpoint_attempts: Vec<Id>,
    pub pending_proposals: Vec<Id>,
    pub blockers: Vec<String>,
    pub history_scope: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchSourceVersion {
    pub source_id: Id,
    pub revision: Revision,
    pub fingerprint: String,
    pub locator: String,
}
impl From<Source> for BranchSourceVersion {
    fn from(s: Source) -> Self {
        Self {
            source_id: s.id,
            revision: s.revision,
            fingerprint: s.fingerprint,
            locator: s.locator,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchLoopReceipt {
    pub original: BranchOpenLoop,
    pub resolution: BranchLoopResolution,
    pub carried_to: Option<ProjectionMeta>,
    pub carry_event_id: Option<Id>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchCloseReceipt {
    pub version: u32,
    pub branch_id: Id,
    pub branch_revision: Revision,
    pub outcome: BranchCloseOutcome,
    pub target_branch_id: Option<Id>,
    pub actor: String,
    pub reason: String,
    pub summary: String,
    pub source_project_revision: Revision,
    pub source_versions: Vec<BranchSourceVersion>,
    pub merge: Option<BranchMergeObservation>,
    pub open_loops: Vec<BranchLoopReceipt>,
    pub retained_session_ids: Vec<Id>,
    pub previous_branch_id: Option<Id>,
    pub current_branch_id: Option<Id>,
    pub summary_event_id: Id,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchClosureRecord {
    pub event_id: Id,
    pub project_revision: Revision,
    pub receipt: BranchCloseReceipt,
}
#[derive(Debug, Clone)]
pub struct CloseBranchDraft {
    pub input: CloseBranchInput,
    pub target_branch_id: Option<Id>,
    pub actor: String,
    pub reason: String,
    pub source_versions: Vec<BranchSourceVersion>,
    pub merge: Option<BranchMergeObservation>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BranchClosed {
    pub branch: Branch,
    pub receipt: BranchCloseReceipt,
}
