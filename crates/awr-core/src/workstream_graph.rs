//! Pure dependency validation. Adapters supply the complete trusted graph within
//! one authenticated tenant/project transaction. This does not prevent concurrent
//! check-then-write races or establish execution authority.
use crate::{WorkstreamCatalog, WorkstreamWorkBinding, validate_workstream_ownership};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const MAX_WORKSTREAM_GRAPH_NODES: usize = 16_384;
pub const MAX_WORKSTREAM_GRAPH_EDGES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DependencyDagError {
    #[error("dependency endpoint is missing")]
    MissingEndpoint,
    /// Directed, closed path (first == last), not the set of blocked nodes.
    #[error("dependency cycle: {0:?}")]
    Cycle(Vec<String>),
}

/// Shared DAG kernel. Duplicate nodes/edges are harmless; every edge is hard.
/// Iterative DFS uses O(V + E) memory and ordered traversal for a deterministic
/// actual cycle, independent of input order. Missing endpoints precede cycles.
/// Adapters enforce size/identity policy; legacy adapters may retain their
/// existing pre-validation error precedence.
pub fn validate_dependency_dag<'a>(
    nodes: impl IntoIterator<Item = &'a str>,
    edges: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(), DependencyDagError> {
    let mut graph: BTreeMap<&str, BTreeSet<&str>> =
        nodes.into_iter().map(|id| (id, BTreeSet::new())).collect();
    for (from, to) in edges {
        if !graph.contains_key(from) || !graph.contains_key(to) {
            return Err(DependencyDagError::MissingEndpoint);
        }
        graph.get_mut(from).expect("checked endpoint").insert(to);
    }
    let mut done = BTreeSet::new();
    let mut active = BTreeMap::new();
    for &root in graph.keys() {
        if done.contains(root) {
            continue;
        }
        let mut path = vec![root];
        active.insert(root, 0);
        let mut stack = vec![graph[root].iter()];
        while let Some(children) = stack.last_mut() {
            if let Some(&child) = children.next() {
                if let Some(&start) = active.get(child) {
                    let mut cycle: Vec<String> =
                        path[start..].iter().map(|id| (*id).into()).collect();
                    cycle.push(child.into());
                    return Err(DependencyDagError::Cycle(cycle));
                }
                if !done.contains(child) {
                    active.insert(child, path.len());
                    path.push(child);
                    stack.push(graph[child].iter());
                }
            } else {
                stack.pop();
                let node = path.pop().expect("one path node per frame");
                active.remove(node);
                done.insert(node);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkstreamDependencyEdge {
    /// Direction is retained verbatim in any reported cycle.
    pub from: WorkstreamWorkBinding,
    pub to: WorkstreamWorkBinding,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkstreamGraphError {
    #[error("workstream graph exceeds its input budget")]
    BudgetExceeded,
    #[error("invalid catalog or incomplete, duplicate, or cross-project ownership")]
    InvalidOwnership,
    #[error("required endpoint does not match the complete project ownership graph")]
    InvalidRequiredEndpoint,
    #[error(transparent)]
    Dependency(#[from] DependencyDagError),
}

/// Validate the complete same-project Work graph, not a graph collapsed to
/// workstreams: A1 -> B1 -> A2 is legal. Catalog and ownership are structural
/// requirements; hard endpoints must match all three ownership identity fields.
/// Reference edges are ignored, including unavailable endpoints and soft cycles;
/// their availability must never become an implicit execution dependency.
/// Input counts (including references) are bounded before allocating graph state.
/// Callers must bound serialized payload bytes before decoding this structure.
pub fn validate_workstream_graph(
    catalog: &WorkstreamCatalog,
    work_ids: &[String],
    ownership: &[WorkstreamWorkBinding],
    edges: &[WorkstreamDependencyEdge],
) -> Result<(), WorkstreamGraphError> {
    if work_ids.len() > MAX_WORKSTREAM_GRAPH_NODES
        || ownership.len() > MAX_WORKSTREAM_GRAPH_NODES
        || edges.len() > MAX_WORKSTREAM_GRAPH_EDGES
    {
        return Err(WorkstreamGraphError::BudgetExceeded);
    }
    if work_ids.iter().any(|id| id.len() > 128)
        || ownership
            .iter()
            .any(|b| b.work_item_id.len() > 128 || b.project_id.len() > 128)
    {
        return Err(WorkstreamGraphError::InvalidOwnership);
    }
    validate_workstream_ownership(catalog, work_ids, ownership)
        .map_err(|_| WorkstreamGraphError::InvalidOwnership)?;
    let known: BTreeMap<_, _> = ownership
        .iter()
        .map(|b| (b.work_item_id.as_str(), b))
        .collect();
    for endpoint in edges
        .iter()
        .filter(|e| e.required)
        .flat_map(|e| [&e.from, &e.to])
    {
        if endpoint.work_item_id.len() > 128
            || endpoint.project_id.len() > 128
            || known.get(endpoint.work_item_id.as_str()).copied() != Some(endpoint)
        {
            return Err(WorkstreamGraphError::InvalidRequiredEndpoint);
        }
    }
    validate_dependency_dag(
        work_ids.iter().map(String::as_str),
        edges
            .iter()
            .filter(|e| e.required)
            .map(|e| (e.from.work_item_id.as_str(), e.to.work_item_id.as_str())),
    )?;
    Ok(())
}
