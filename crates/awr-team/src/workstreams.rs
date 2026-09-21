//! Explicit multi-work source codec. Work branches keep their existing scope ID;
//! workstream ownership is a separate, source-backed dimension.
use crate::{TeamError, TeamResult, WorkContract, contract_hash};
use awr_core::{Id, WorkstreamCatalog, WorkstreamWorkBinding, validate_workstream_ownership};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamContract {
    pub workstream_id: Id,
    pub contract: WorkContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamBundle {
    pub codec: String,
    pub catalog: WorkstreamCatalog,
    pub contracts: Vec<WorkstreamContract>,
}

impl WorkstreamBundle {
    pub const CODEC: &'static str = "awr-team-workstreams-v1";

    pub fn validate(&self, project_id: &str) -> TeamResult<()> {
        if self.codec != Self::CODEC || self.catalog.project_id != project_id {
            return Err(TeamError::InvalidContract(
                "workstream codec or project mismatch".into(),
            ));
        }
        if self.contracts.is_empty() || self.contracts.len() > 10_000 {
            return Err(TeamError::InvalidContract(
                "workstream source needs 1..10000 contracts".into(),
            ));
        }
        let mut keys = BTreeSet::new();
        for entry in &self.contracts {
            entry.contract.validate()?;
            if !keys.insert(&entry.contract.external_key) {
                return Err(TeamError::InvalidContract("duplicate work key".into()));
            }
        }
        let ids = self
            .contracts
            .iter()
            .map(|e| e.contract.work_id.as_str().to_owned())
            .collect::<Vec<_>>();
        let bindings = self
            .contracts
            .iter()
            .map(|e| WorkstreamWorkBinding {
                project_id: project_id.into(),
                work_item_id: e.contract.work_id.as_str().into(),
                workstream_id: e.workstream_id,
            })
            .collect::<Vec<_>>();
        validate_workstream_ownership(&self.catalog, &ids, &bindings)
            .map_err(|e| TeamError::InvalidContract(e.to_string()))
    }

    /// A projection identity, not an individual work's contract hash. Preserve
    /// each nested V1 hash so migration cannot silently reinterpret old receipts.
    pub fn hash(&self) -> TeamResult<String> {
        self.validate(&self.catalog.project_id)?;
        let mut catalog = self.catalog.clone();
        catalog.workstreams.sort_by_key(|s| s.id);
        for stream in &mut catalog.workstreams {
            stream.goal_keys.sort();
            stream.acceptance_contracts.sort();
        }
        let mut contracts = self
            .contracts
            .iter()
            .map(|e| {
                Ok((
                    e.contract.work_id.as_str(),
                    e.workstream_id,
                    e.contract.hash()?,
                ))
            })
            .collect::<TeamResult<Vec<_>>>()?;
        contracts.sort();
        contract_hash(&json!({"codec":Self::CODEC,"catalog":catalog,"contracts":contracts}))
    }
}
