//! Workstream identity and scope rules, independent of storage or a transport.
//! Callers supply current, trusted catalog/ownership/permission records. These
//! functions do not authenticate a request, load grants, or lock a transaction.
//! Existing project/work/session IDs are opaque strings and are never remapped.
//! Multi-tenant adapters must load all records within the authenticated tenant;
//! a project ID alone is not a globally unique tenant authorization boundary.
use crate::{Id, Revision};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const WORKSTREAM_CATALOG_VERSION: u32 = 1;
pub const MAX_WORKSTREAMS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamState {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workstream {
    pub id: Id,
    pub project_id: String,
    pub external_key: String,
    pub title: String,
    pub state: WorkstreamState,
    /// Changes to scope authority, not the number of ordinary work updates.
    pub authority_version: Revision,
    pub goal_keys: Vec<String>,
    pub acceptance_contracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamCatalog {
    pub version: u32,
    pub project_id: String,
    /// An explicit compatibility binding, never a multi-scope selection fallback.
    pub legacy_default: Option<Id>,
    pub workstreams: Vec<Workstream>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkstreamError {
    #[error("unsupported workstream catalog version {0}")]
    UnsupportedVersion(u32),
    #[error("invalid workstream definition: {0}")]
    InvalidDefinition(&'static str),
    #[error("an explicit workstream is required")]
    ScopeRequired,
    #[error("workstream, work and session bindings do not agree")]
    BindingMismatch,
    #[error("workstream access denied")]
    AccessDenied,
    #[error("workstream authority has changed; refresh the permission binding")]
    StaleAuthority,
    #[error("workstream is unavailable")]
    Unavailable,
    #[error("ordinary writes require an active workstream")]
    Inactive,
}

impl WorkstreamError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedVersion(_) => "WorkstreamProtocolUnsupported",
            Self::InvalidDefinition(_) => "WorkstreamInvalidDefinition",
            Self::ScopeRequired => "WorkstreamScopeRequired",
            Self::BindingMismatch => "WorkstreamBindingMismatch",
            Self::AccessDenied => "WorkstreamAccessDenied",
            Self::StaleAuthority => "WorkstreamStaleAuthority",
            Self::Unavailable => "WorkstreamUnavailable",
            Self::Inactive => "WorkstreamInactive",
        }
    }
}

pub type WorkstreamResult<T> = std::result::Result<T, WorkstreamError>;

fn non_nil(id: Id) -> bool {
    u128::from(id) != 0
}

// Matches legacy Team identifiers, including significant whitespace. Local
// ULIDs use their existing string representation; do not normalize either kind.
fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn bounded_text(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= limit
        && !value.chars().any(char::is_control)
}

fn references(values: &[String]) -> bool {
    values.len() <= 256
        && values.iter().all(|v| bounded_text(v, 4096))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

impl Workstream {
    pub fn validate(&self) -> WorkstreamResult<()> {
        let key = self.external_key.as_bytes();
        if !non_nil(self.id)
            || !valid_identity(&self.project_id)
            || key.is_empty()
            || key.len() > 128
            || !key[0].is_ascii_alphanumeric()
            || !key
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b))
            || !bounded_text(&self.title, 512)
            || self.authority_version == 0
            || !references(&self.goal_keys)
            || !references(&self.acceptance_contracts)
        {
            return Err(WorkstreamError::InvalidDefinition(
                "identity, key, title, authority version or references are invalid",
            ));
        }
        Ok(())
    }

    /// Display edits do not change scope identity. Authority changes invalidate
    /// old permission proofs; storage must enforce this check atomically.
    pub fn validate_successor(&self, previous: &Self) -> WorkstreamResult<()> {
        self.validate()?;
        previous.validate()?;
        if self.id != previous.id
            || self.project_id != previous.project_id
            || self.external_key != previous.external_key
        {
            return Err(WorkstreamError::BindingMismatch);
        }
        let authority_changed = self.state != previous.state
            || self.goal_keys.iter().collect::<BTreeSet<_>>()
                != previous.goal_keys.iter().collect::<BTreeSet<_>>()
            || self.acceptance_contracts.iter().collect::<BTreeSet<_>>()
                != previous
                    .acceptance_contracts
                    .iter()
                    .collect::<BTreeSet<_>>();
        if self.authority_version < previous.authority_version
            || (authority_changed && self.authority_version == previous.authority_version)
        {
            return Err(WorkstreamError::StaleAuthority);
        }
        Ok(())
    }
}

impl WorkstreamCatalog {
    pub fn validate(&self) -> WorkstreamResult<()> {
        if self.version != WORKSTREAM_CATALOG_VERSION {
            return Err(WorkstreamError::UnsupportedVersion(self.version));
        }
        if !valid_identity(&self.project_id)
            || self.workstreams.is_empty()
            || self.workstreams.len() > MAX_WORKSTREAMS
        {
            return Err(WorkstreamError::InvalidDefinition(
                "catalog requires a project and a bounded nonempty scope set",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for stream in &self.workstreams {
            stream.validate()?;
            if stream.project_id != self.project_id {
                return Err(WorkstreamError::BindingMismatch);
            }
            if !ids.insert(stream.id) || !keys.insert(&stream.external_key) {
                return Err(WorkstreamError::InvalidDefinition(
                    "duplicate workstream identity or key",
                ));
            }
        }
        if self.legacy_default.is_some_and(|id| !ids.contains(&id)) {
            return Err(WorkstreamError::InvalidDefinition(
                "legacy default is not a declared workstream",
            ));
        }
        Ok(())
    }

    /// Stable synthetic identity for a legacy project's one scope. This is a
    /// compatibility mapping only; it neither writes sources nor grants access.
    pub fn legacy(project_id: impl Into<String>) -> WorkstreamResult<Self> {
        let project_id = project_id.into();
        let mut hash = Sha256::new();
        hash.update(b"awr:legacy-workstream:v1:");
        hash.update(project_id.as_bytes());
        let digest = hash.finalize();
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        let id = Id::from(u128::from_be_bytes(bytes));
        let catalog = Self {
            version: WORKSTREAM_CATALOG_VERSION,
            project_id: project_id.clone(),
            legacy_default: Some(id),
            workstreams: vec![Workstream {
                id,
                project_id,
                external_key: "main".into(),
                title: "Main".into(),
                state: WorkstreamState::Active,
                authority_version: 1,
                goal_keys: vec![],
                acceptance_contracts: vec![],
            }],
        };
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn get(&self, id: Id) -> WorkstreamResult<&Workstream> {
        self.workstreams
            .iter()
            .find(|stream| stream.id == id)
            .ok_or(WorkstreamError::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamWorkBinding {
    pub project_id: String,
    pub workstream_id: Id,
    pub work_item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamSessionBinding {
    pub work: WorkstreamWorkBinding,
    pub session_id: String,
}

/// A complete source projection, not caller-supplied execution authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamProjection {
    pub catalog: WorkstreamCatalog,
    pub ownership: Vec<WorkstreamWorkBinding>,
}

fn validate_work_binding(
    catalog: &WorkstreamCatalog,
    binding: &WorkstreamWorkBinding,
) -> WorkstreamResult<()> {
    if binding.project_id != catalog.project_id || !valid_identity(&binding.work_item_id) {
        return Err(WorkstreamError::BindingMismatch);
    }
    catalog.get(binding.workstream_id)?;
    Ok(())
}

/// Validate the complete ownership set supplied by a source/index transaction.
/// This does not permit moving a live work; that requires runtime checks too.
pub fn validate_workstream_ownership(
    catalog: &WorkstreamCatalog,
    work_ids: &[String],
    bindings: &[WorkstreamWorkBinding],
) -> WorkstreamResult<()> {
    catalog.validate()?;
    let expected = work_ids.iter().collect::<BTreeSet<_>>();
    if expected.len() != work_ids.len() || work_ids.iter().any(|id| !valid_identity(id)) {
        return Err(WorkstreamError::InvalidDefinition(
            "invalid work identity set",
        ));
    }
    let mut seen = BTreeSet::new();
    for binding in bindings {
        validate_work_binding(catalog, binding)?;
        if !seen.insert(&binding.work_item_id) {
            return Err(WorkstreamError::InvalidDefinition(
                "work must have exactly one owning workstream",
            ));
        }
    }
    if seen != expected {
        return Err(WorkstreamError::InvalidDefinition(
            "ownership must cover exactly the declared work set",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkstreamAction {
    Read,
    Write,
    Manage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamGrant {
    pub workstream_id: Id,
    pub authority_version: Revision,
    pub read: bool,
    pub write: bool,
    pub manage: bool,
}

impl WorkstreamGrant {
    fn permits(&self, action: WorkstreamAction) -> bool {
        match action {
            WorkstreamAction::Read => self.read,
            WorkstreamAction::Write => self.write,
            WorkstreamAction::Manage => self.manage,
        }
    }
}

/// Construct from authenticated service policy, never from request arguments.
/// Deliberately not Deserialize: a transport cannot decode this as caller proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamAccess {
    pub project_id: String,
    pub subject: String,
    pub grants: Vec<WorkstreamGrant>,
}

impl WorkstreamAccess {
    pub fn validate(&self) -> WorkstreamResult<()> {
        let mut seen = BTreeSet::new();
        if !valid_identity(&self.project_id)
            || !bounded_text(&self.subject, 512)
            || self.grants.len() > MAX_WORKSTREAMS
            || self.grants.iter().any(|grant| {
                !non_nil(grant.workstream_id)
                    || grant.authority_version == 0
                    || ((grant.write || grant.manage) && !grant.read)
                    || !seen.insert(grant.workstream_id)
            })
        {
            return Err(WorkstreamError::InvalidDefinition(
                "invalid permission identity, duplicate scope or incomplete read grant",
            ));
        }
        Ok(())
    }

    pub fn authorize(
        &self,
        catalog: &WorkstreamCatalog,
        id: Id,
        action: WorkstreamAction,
    ) -> WorkstreamResult<()> {
        // Validate structural policy, but do not invalidate an unrelated grant
        // merely because another workstream's authority version changed.
        self.validate()?;
        catalog.validate()?;
        self.authorize_validated(catalog, id, action)
    }

    fn authorize_validated(
        &self,
        catalog: &WorkstreamCatalog,
        id: Id,
        action: WorkstreamAction,
    ) -> WorkstreamResult<()> {
        if self.project_id != catalog.project_id {
            return Err(WorkstreamError::AccessDenied);
        }
        let grant = self
            .grants
            .iter()
            .find(|grant| grant.workstream_id == id)
            .ok_or(WorkstreamError::AccessDenied)?;
        if !grant.permits(action) {
            return Err(WorkstreamError::AccessDenied);
        }
        let stream = catalog.get(id)?;
        if grant.authority_version != stream.authority_version {
            return Err(WorkstreamError::StaleAuthority);
        }
        if action == WorkstreamAction::Write && stream.state != WorkstreamState::Active {
            return Err(WorkstreamError::Inactive);
        }
        Ok(())
    }
}

/// Work/session values are looked up by the trusted runtime. Only selectors,
/// not ownership or grant records, may originate in an external request.
#[derive(Debug, Clone, Default)]
pub struct WorkstreamSelection {
    pub explicit: Option<Id>,
    pub work: Option<WorkstreamWorkBinding>,
    pub session: Option<WorkstreamSessionBinding>,
    pub conversation_default: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkstreamSelectionBasis {
    Work,
    Session,
    Explicit,
    Conversation,
    UniqueAuthorized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedWorkstream {
    pub project_id: String,
    pub workstream_id: Id,
    pub work_item_id: Option<String>,
    pub session_id: Option<String>,
    pub authority_version: Revision,
    pub basis: WorkstreamSelectionBasis,
}

/// Resolve against one coherent snapshot. The result is not a reusable grant:
/// dispatch and writes must recheck current authority and execution conditions.
pub fn resolve_workstream(
    catalog: &WorkstreamCatalog,
    access: &WorkstreamAccess,
    selection: &WorkstreamSelection,
    action: WorkstreamAction,
) -> WorkstreamResult<ResolvedWorkstream> {
    catalog.validate()?;
    access.validate()?;
    if access.project_id != catalog.project_id {
        return Err(WorkstreamError::AccessDenied);
    }
    if let Some(session) = &selection.session {
        if !valid_identity(&session.session_id)
            || selection
                .work
                .as_ref()
                .is_some_and(|work| work != &session.work)
        {
            return Err(WorkstreamError::BindingMismatch);
        }
    }
    let binding = selection
        .session
        .as_ref()
        .map(|session| &session.work)
        .or(selection.work.as_ref());
    let (id, basis) = if let Some(work) = binding {
        if work.project_id != catalog.project_id
            || selection
                .explicit
                .is_some_and(|id| id != work.workstream_id)
        {
            return Err(WorkstreamError::BindingMismatch);
        }
        (
            work.workstream_id,
            if selection.session.is_some() {
                WorkstreamSelectionBasis::Session
            } else {
                WorkstreamSelectionBasis::Work
            },
        )
    } else if let Some(id) = selection.explicit {
        (id, WorkstreamSelectionBasis::Explicit)
    } else if let Some(id) = selection.conversation_default {
        (id, WorkstreamSelectionBasis::Conversation)
    } else {
        // Do not guess another scope when an existing permission becomes stale
        // or a stream pauses. Such changes must not change implicit ownership.
        let grants = access
            .grants
            .iter()
            .map(|grant| (grant.workstream_id, grant))
            .collect::<BTreeMap<_, _>>();
        let candidates = catalog
            .workstreams
            .iter()
            .filter(|stream| {
                grants
                    .get(&stream.id)
                    .is_some_and(|grant| grant.permits(action))
            })
            .map(|stream| stream.id)
            .take(2)
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [id] => (*id, WorkstreamSelectionBasis::UniqueAuthorized),
            [] => return Err(WorkstreamError::AccessDenied),
            _ => return Err(WorkstreamError::ScopeRequired),
        }
    };
    access.authorize_validated(catalog, id, action)?;
    if let Some(work) = binding {
        validate_work_binding(catalog, work)?;
    }
    Ok(ResolvedWorkstream {
        project_id: catalog.project_id.clone(),
        workstream_id: id,
        work_item_id: binding.map(|work| work.work_item_id.clone()),
        session_id: selection
            .session
            .as_ref()
            .map(|session| session.session_id.clone()),
        authority_version: catalog.get(id)?.authority_version,
        basis,
    })
}
