use crate::error::{PgError, PgResult};
use crate::tx::{bind_scope, new_id};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SplitProposal {
    pub id: String,
    pub parent_work_id: String,
    pub child_work_ids: Vec<String>,
}

pub fn paths_conflict(kind_a: &str, key_a: &str, kind_b: &str, key_b: &str) -> bool {
    if kind_a == "named" || kind_b == "named" {
        return kind_a == kind_b && key_a == key_b;
    }
    if kind_a == "file" && kind_b == "file" {
        return canonicalize(key_a) == canonicalize(key_b);
    }
    segment_prefix_overlap(&canonicalize(key_a), &canonicalize(key_b))
}

fn canonicalize(path: &str) -> String {
    // Normalize repeated separators and '.' segments so aliases of one file
    // cannot bypass the conflict check (CR #40 P2-2). '..' stays visible
    // here; it is rejected at the reserve entry.
    path.replace('\\', "/")
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Entry-level resource key validation/normalization. file/prefix resources
/// are normalized to their canonical workspace-relative form; named
/// resources keep their own identity rules and are not path-processed
/// (CR #40 P2-2).
fn normalize_resource_key(kind: &str, key: &str) -> PgResult<String> {
    if kind == "named" {
        if key.is_empty() {
            return Err(PgError::UnsafeSourcePath("empty named resource".into()));
        }
        return Ok(key.to_string());
    }
    // Unify separators BEFORE any segment check: a backslash '..' must not
    // become a parent segment only after validation (CR #57 P2-1). The same
    // normalized form is then checked, compared and stored.
    let normalized = key.replace('\\', "/");
    if normalized.split('/').any(|segment| segment == "..") {
        return Err(PgError::UnsafeSourcePath(key.into()));
    }
    let canonical = canonicalize(&normalized);
    if canonical.is_empty() {
        return Err(PgError::UnsafeSourcePath(key.into()));
    }
    Ok(canonical)
}

fn segment_prefix_overlap(a: &str, b: &str) -> bool {
    let aa = a.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let bb = b.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>();
    if aa.is_empty() || bb.is_empty() {
        return false;
    }
    let n = aa.len().min(bb.len());
    aa[..n] == bb[..n]
}

pub fn validate_required_graph(nodes: &[String], edges: &[DependencyEdge]) -> PgResult<()> {
    let known: HashSet<&str> = nodes.iter().map(|n| n.as_str()).collect();
    // Preserve legacy input-order precedence for missing endpoints/self loops,
    // duplicate node identities and ignored non-required edges.
    for edge in edges.iter().filter(|e| e.required) {
        if !known.contains(edge.from.as_str()) || !known.contains(edge.to.as_str()) {
            return Err(PgError::MissingDependency);
        }
        if edge.from == edge.to {
            return Err(PgError::DependencyCycle);
        }
    }
    awr_core::validate_dependency_dag(
        nodes.iter().map(String::as_str),
        edges
            .iter()
            .filter(|e| e.required)
            .map(|e| (e.from.as_str(), e.to.as_str())),
    )
    .map_err(|error| match error {
        awr_core::DependencyDagError::MissingEndpoint => PgError::MissingDependency,
        awr_core::DependencyDagError::Cycle(_) => PgError::DependencyCycle,
    })
}

/// Directional containment for scope authorization: `path` must be the
/// granted file itself or inside a granted directory prefix, segment-wise.
/// This is NOT the symmetric overlap used for reservation conflicts —
/// `src` is NOT inside `src/foo` (CR #41 P2-10).
pub fn path_within_scope(declared: &str, path: &str) -> bool {
    // Both sides must be safe relative forms: a path with parent components
    // (src/foo/../bar) or an absolute path is never "within" (CR #58 P2-6).
    fn safe_form(raw: &str) -> Option<String> {
        let normalized = raw.replace('\\', "/");
        if normalized.starts_with('/') || normalized.split('/').any(|s| s == "..") {
            return None;
        }
        Some(canonicalize(&normalized))
    }
    let (Some(declared), Some(path)) = (safe_form(declared), safe_form(path)) else {
        return false;
    };
    path == declared || path.starts_with(&format!("{declared}/"))
}

pub fn require_main_scope(scope_id: &str) -> PgResult<()> {
    if scope_id != "main" {
        return Err(PgError::ScopeUnsupported);
    }
    Ok(())
}

/// Serialize resource/graph writes on the project coordination row: the
/// check-then-write sequence in reserve/replace_edges/split is only safe
/// when every writer follows the same protocol (CR #40 P2-1, P2-3). An empty
/// reservation set has no rows to lock, so the lock must not live on the
/// reservations themselves.
async fn lock_project(
    tx: &tokio_postgres::Transaction<'_>,
    tenant_id: &str,
    project_id: &str,
) -> PgResult<()> {
    crate::tx::lock_active_project(tx, tenant_id, project_id).await
}

pub struct GraphStore {
    pool: crate::PgPool,
}

impl GraphStore {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            pool: crate::PgPool::new(url),
        }
    }

    /// Build from a validated `tokio_postgres::Config` (see PgPool::from_config).
    pub fn from_config(config: tokio_postgres::Config) -> Self {
        Self {
            pool: crate::PgPool::from_config(config),
        }
    }

    async fn connect(&self) -> PgResult<crate::PgClient> {
        self.pool.get().await
    }

    pub async fn replace_edges(
        &self,
        tenant_id: &str,
        project_id: &str,
        snapshot_id: &str,
        scope_id: &str,
        nodes: &[String],
        edges: &[DependencyEdge],
    ) -> PgResult<()> {
        require_main_scope(scope_id)?;
        validate_required_graph(nodes, edges)?;
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        // Two acyclic replacements must not commit a cyclic union: writers
        // serialize on the project row, so the last writer replaces the
        // committed graph wholesale instead of merging (CR #40 P2-3).
        lock_project(&tx, tenant_id, project_id).await?;
        tx.execute(
            "DELETE FROM awr_team.dependency_edges
             WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id=$4",
            &[&tenant_id, &project_id, &snapshot_id, &scope_id],
        )
        .await?;
        for edge in edges {
            tx.execute(
                "INSERT INTO awr_team.dependency_edges(
                    tenant_id, project_id, snapshot_id, scope_id, from_work_id, to_work_id,
                    relation, required)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                &[
                    &tenant_id,
                    &project_id,
                    &snapshot_id,
                    &scope_id,
                    &edge.from,
                    &edge.to,
                    &edge.relation,
                    &edge.required,
                ],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn reserve(
        &self,
        tenant_id: &str,
        project_id: &str,
        work_id: &str,
        kind: &str,
        key: &str,
    ) -> PgResult<String> {
        let key = normalize_resource_key(kind, key)?;
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        // The conflict check and the insert must be one serialized business
        // operation: without the project lock, two conflicting reservations
        // can both pass the check and both commit (CR #40 P2-1).
        lock_project(&tx, tenant_id, project_id).await?;
        let rows = tx
            .query(
                "SELECT resource_kind, canonical_key FROM awr_team.resource_reservations
                 WHERE tenant_id=$1 AND project_id=$2 AND state IN ('reserved', 'unknown')",
                &[&tenant_id, &project_id],
            )
            .await?;
        for row in rows {
            let existing_kind: String = row.get(0);
            let existing_key: String = row.get(1);
            if paths_conflict(kind, &key, &existing_kind, &existing_key) {
                return Err(PgError::ResourceConflict);
            }
        }
        let id = new_id();
        tx.execute(
            "INSERT INTO awr_team.resource_reservations(
                tenant_id, project_id, id, work_id, resource_kind, canonical_key, state)
             VALUES ($1,$2,$3,$4,$5,$6,'reserved')",
            &[&tenant_id, &project_id, &id, &work_id, &kind, &key],
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn propose_split(
        &self,
        tenant_id: &str,
        project_id: &str,
        parent_work_id: &str,
        children: &[String],
        mapping: &Value,
    ) -> PgResult<SplitProposal> {
        if children.is_empty() {
            return Err(PgError::Protocol("split requires children".into()));
        }
        // Identity rules: children must be new, unique, and different from
        // the parent. Reusing an existing id would either skip the contract
        // inheritance (ON CONFLICT DO NOTHING) or write a required self-loop
        // edge (CR #40 P2-5).
        let unique: HashSet<&str> = children.iter().map(|c| c.as_str()).collect();
        if unique.len() != children.len() {
            return Err(PgError::Protocol("split children must be unique".into()));
        }
        if unique.contains(parent_work_id) {
            return Err(PgError::Protocol(
                "split children must differ from the parent".into(),
            ));
        }
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let existing: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.work_items
                 WHERE tenant_id=$1 AND project_id=$2 AND id = ANY($3)",
                &[
                    &tenant_id,
                    &project_id,
                    &children.iter().collect::<Vec<_>>(),
                ],
            )
            .await?
            .get(0);
        if existing > 0 {
            return Err(PgError::Protocol(
                "split children must be new work ids".into(),
            ));
        }
        let parent = tx
            .query_opt(
                "SELECT c.contract_json FROM awr_team.work_contracts c
                 JOIN awr_team.projects p
                   ON p.tenant_id=c.tenant_id AND p.id=c.project_id
                  AND p.active_snapshot_id=c.snapshot_id
                 WHERE c.tenant_id=$1 AND c.project_id=$2 AND c.work_id=$3",
                &[&tenant_id, &project_id, &parent_work_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("parent contract missing".into()))?;
        let parent_json: Value = parent.get(0);
        let snapshot: String = tx
            .query_one(
                "SELECT active_snapshot_id FROM awr_team.projects
                 WHERE tenant_id=$1 AND id=$2",
                &[&tenant_id, &project_id],
            )
            .await?
            .get(0);
        let mut split_edges = Vec::new();
        for child in children {
            tx.execute(
                "INSERT INTO awr_team.work_items(tenant_id, project_id, id, external_key)
                 VALUES ($1,$2,$3,$3)",
                &[&tenant_id, &project_id, &child],
            )
            .await?;
            // Child contracts are REAL contracts: inherit the parent content
            // with the child identity, re-validate and re-hash. Storing the
            // raw child id as the hash (the old behavior) made the hash both
            // unreproducible and content-free (CR #40 P2-4).
            let mut child_json = parent_json.clone();
            if let Some(obj) = child_json.as_object_mut() {
                obj.insert("work_id".into(), json!(child));
                obj.insert("external_key".into(), json!(child));
            }
            let child_contract: awr_team::WorkContract = serde_json::from_value(child_json.clone())
                .map_err(|e| PgError::Protocol(format!("invalid inherited child contract: {e}")))?;
            let child_hash = child_contract
                .hash()
                .map_err(|e| PgError::Protocol(e.to_string()))?;
            let stored_json = serde_json::to_value(&child_contract)
                .map_err(|e| PgError::Protocol(e.to_string()))?;
            let title = child_contract.external_key.clone();
            tx.execute(
                "INSERT INTO awr_team.work_contracts(
                    tenant_id, project_id, snapshot_id, scope_id, work_id,
                    contract_hash, definition_state, title, contract_json)
                 VALUES ($1,$2,$3,'main',$4,$5,'enabled',$6,$7)",
                &[
                    &tenant_id,
                    &project_id,
                    &snapshot,
                    &child,
                    &child_hash,
                    &title,
                    &stored_json,
                ],
            )
            .await?;
            tx.execute(
                "INSERT INTO awr_team.dependency_edges(
                    tenant_id, project_id, snapshot_id, scope_id, from_work_id, to_work_id,
                    relation, required)
                 VALUES ($1,$2,$3,'main',$4,$5,'split-child',true)
                 ON CONFLICT DO NOTHING",
                &[&tenant_id, &project_id, &snapshot, &parent_work_id, &child],
            )
            .await?;
            split_edges.push(DependencyEdge {
                from: parent_work_id.into(),
                to: child.clone(),
                relation: "split-child".into(),
                required: true,
            });
        }
        // The graph AFTER adding the split edges must still be valid.
        let mut nodes: Vec<String> = vec![parent_work_id.to_string()];
        nodes.extend(children.iter().cloned());
        let existing_edges: Vec<DependencyEdge> = tx
            .query(
                "SELECT from_work_id, to_work_id, relation, required
                 FROM awr_team.dependency_edges
                 WHERE tenant_id=$1 AND project_id=$2 AND snapshot_id=$3 AND scope_id='main'",
                &[&tenant_id, &project_id, &snapshot],
            )
            .await?
            .iter()
            .map(|row| DependencyEdge {
                from: row.get(0),
                to: row.get(1),
                relation: row.get(2),
                required: row.get(3),
            })
            .collect();
        let mut all_nodes = nodes;
        for edge in &existing_edges {
            if !all_nodes.contains(&edge.from) {
                all_nodes.push(edge.from.clone());
            }
            if !all_nodes.contains(&edge.to) {
                all_nodes.push(edge.to.clone());
            }
        }
        validate_required_graph(&all_nodes, &existing_edges)?;
        let id = new_id();
        let child_json = json!(children);
        tx.execute(
            "INSERT INTO awr_team.split_proposals(
                tenant_id, project_id, id, parent_work_id, child_work_ids, mapping_json, state)
             VALUES ($1,$2,$3,$4,$5,$6,'accepted')",
            &[
                &tenant_id,
                &project_id,
                &id,
                &parent_work_id,
                &child_json,
                mapping,
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(SplitProposal {
            id,
            parent_work_id: parent_work_id.into(),
            child_work_ids: children.to_vec(),
        })
    }

    pub async fn complete_parent_from_children(
        &self,
        tenant_id: &str,
        project_id: &str,
        parent_work_id: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let _proposal = tx
            .query_opt(
                "SELECT id FROM awr_team.split_proposals
                 WHERE tenant_id=$1 AND project_id=$2 AND parent_work_id=$3 AND state='accepted'",
                &[&tenant_id, &project_id, &parent_work_id],
            )
            .await?
            .ok_or_else(|| PgError::Protocol("split proposal missing".into()))?;
        tx.commit().await?;
        Err(PgError::ParentEvidenceRequired)
    }

    pub async fn bind_dependency(
        &self,
        tenant_id: &str,
        project_id: &str,
        downstream: &str,
        upstream: &str,
        binding_hash: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        tx.execute(
            "INSERT INTO awr_team.dependency_bindings(
                tenant_id, project_id, downstream_work_id, upstream_work_id, binding_hash, valid)
             VALUES ($1,$2,$3,$4,$5,true)
             ON CONFLICT (tenant_id, project_id, downstream_work_id, upstream_work_id)
             DO UPDATE SET binding_hash=$5, valid=true",
            &[
                &tenant_id,
                &project_id,
                &downstream,
                &upstream,
                &binding_hash,
            ],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn invalidate_downstream(
        &self,
        tenant_id: &str,
        project_id: &str,
        upstream: &str,
    ) -> PgResult<u64> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        lock_project(&tx, tenant_id, project_id).await?;
        let count = tx
            .execute(
                "UPDATE awr_team.dependency_bindings SET valid=false
                 WHERE tenant_id=$1 AND project_id=$2 AND upstream_work_id=$3 AND valid=true",
                &[&tenant_id, &project_id, &upstream],
            )
            .await?;
        tx.commit().await?;
        Ok(count)
    }

    pub async fn current_binding_valid(
        &self,
        tenant_id: &str,
        project_id: &str,
        downstream: &str,
        upstream: &str,
    ) -> PgResult<bool> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let row = tx
            .query_opt(
                "SELECT valid FROM awr_team.dependency_bindings
                 WHERE tenant_id=$1 AND project_id=$2 AND downstream_work_id=$3 AND upstream_work_id=$4",
                &[&tenant_id, &project_id, &downstream, &upstream],
            )
            .await?;
        tx.commit().await?;
        match row {
            Some(row) => Ok(row.get(0)),
            None => Err(PgError::BindingInvalid),
        }
    }

    pub async fn activation_blocked_by_claims(
        &self,
        tenant_id: &str,
        project_id: &str,
        work_id: &str,
        new_contract_hash: &str,
    ) -> PgResult<()> {
        let mut client = self.connect().await?;
        let tx = client.transaction().await?;
        bind_scope(&tx, tenant_id, project_id).await?;
        let claimed: i64 = tx
            .query_one(
                "SELECT count(*) FROM awr_team.claims
                 WHERE tenant_id=$1 AND project_id=$2 AND work_id=$3 AND state='active'
                   AND expires_at > clock_timestamp()",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?
            .get(0);
        if claimed == 0 {
            tx.commit().await?;
            return Ok(());
        }
        let current = tx
            .query_opt(
                "SELECT c.contract_hash FROM awr_team.work_contracts c
                 JOIN awr_team.projects p
                   ON p.tenant_id=c.tenant_id AND p.id=c.project_id
                  AND p.active_snapshot_id=c.snapshot_id
                 WHERE c.tenant_id=$1 AND c.project_id=$2 AND c.work_id=$3",
                &[&tenant_id, &project_id, &work_id],
            )
            .await?;
        tx.commit().await?;
        if let Some(row) = current {
            let hash: String = row.get(0);
            if hash != new_contract_hash {
                return Err(PgError::ClaimBlocksActivation);
            }
        }
        Ok(())
    }

    pub async fn graph_within_budget(
        &self,
        edges: &[DependencyEdge],
        max_edges: usize,
    ) -> PgResult<()> {
        if edges.len() > max_edges {
            return Err(PgError::GraphBudgetExceeded);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_graph_semantics_survive_kernel_extraction() {
        let nodes = vec!["a".into(), "a".into(), "b".into()];
        let edge = |from: &str, to: &str, required| DependencyEdge {
            from: from.into(),
            to: to.into(),
            relation: "arbitrary".into(),
            required,
        };
        assert!(
            validate_required_graph(
                &nodes,
                &[
                    edge("a", "b", true),
                    edge("a", "b", true),
                    edge("b", "a", false),
                    edge("missing", "missing", false)
                ]
            )
            .is_ok()
        );
        assert!(matches!(
            validate_required_graph(&nodes, &[edge("a", "a", true), edge("a", "missing", true)]),
            Err(PgError::DependencyCycle)
        ));
        assert!(matches!(
            validate_required_graph(&nodes, &[edge("a", "missing", true), edge("a", "a", true)]),
            Err(PgError::MissingDependency)
        ));
        assert!(validate_required_graph(&[], &[]).is_ok());
    }

    #[test]
    fn prefix_overlaps_segment_wise_not_string_prefix() {
        assert!(paths_conflict(
            "prefix",
            "src/foo",
            "file",
            "src/foo/bar.rs"
        ));
        assert!(!paths_conflict("prefix", "src/a", "file", "src/abc"));
        assert!(paths_conflict("file", "src/a.rs", "file", "src/a.rs"));
        assert!(!paths_conflict("named", "lock-a", "named", "lock-b"));
    }

    #[test]
    fn required_cycle_and_missing_nodes_are_rejected() {
        let nodes = vec!["a".into(), "b".into()];
        let cycle = vec![
            DependencyEdge {
                from: "a".into(),
                to: "b".into(),
                relation: "requires".into(),
                required: true,
            },
            DependencyEdge {
                from: "b".into(),
                to: "a".into(),
                relation: "requires".into(),
                required: true,
            },
        ];
        assert!(matches!(
            validate_required_graph(&nodes, &cycle),
            Err(PgError::DependencyCycle)
        ));
        let missing = vec![DependencyEdge {
            from: "a".into(),
            to: "z".into(),
            relation: "requires".into(),
            required: true,
        }];
        assert!(matches!(
            validate_required_graph(&nodes, &missing),
            Err(PgError::MissingDependency)
        ));
        assert!(require_main_scope("feature").is_err());
        assert!(require_main_scope("main").is_ok());
    }
}
