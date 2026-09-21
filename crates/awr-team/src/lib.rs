//! Team V1 domain contracts and pure rules.
//!
//! This crate has no database driver. SQLite and PostgreSQL adapters consume
//! these types; they do not own contract hashing or completion semantics.
mod access;
mod canonical;
mod completion;
mod contract;
mod error;
mod ids;
mod snapshot;
mod version;
mod workstreams;

pub use access::{
    AuthContext, Envelope, RemoteProfile, SURFACES, authorize, execute, parse_envelope,
    same_error_on_all_surfaces, validate_only,
};
pub use canonical::{
    HASH_CODEC, canonical_json, contract_hash, reject_unknown_required_fields, request_hash,
};
pub use completion::{
    CompletionView, EvidenceBundle, EvidenceGrade, ReviewPolicy, current_completion,
};
pub use contract::{WorkContract, WorkDefinitionState};
pub use error::{TeamError, TeamResult};
pub use ids::{ActorId, ProjectId, RequestId, ScopeId, SessionId, TenantId, WorkId};
pub use snapshot::{
    ClaimPreconditions, LeaseProof, ProjectReadSnapshot, RequiredDependencyProof,
    SourceActivationPlan,
};
pub use version::{decode_u64, encode_u64};
pub use workstreams::{WorkstreamBundle, WorkstreamContract};

pub const PROTOCOL: &str = "awr-team";
pub const PROTOCOL_VERSION: u32 = 1;
