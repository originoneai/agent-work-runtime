//! An explicitly authorized, immutable read snapshot. The underlying Store is
//! intentionally not exposed: its ordinary APIs are trusted project-wide APIs.
use crate::{
    CatalogCursor, CatalogKind, CatalogPage, CatalogScope, EventCursor, EventPage, EventQuery,
    SearchQuery, SearchReport, Store, db_error, scope_membership,
};
use awr_core::*;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Only selectors come from the request. Access must come from trusted policy.
#[derive(Debug, Clone, Default)]
pub struct WorkstreamReadSelection {
    pub workstream_id: Option<Id>,
    pub work_item_key: Option<String>,
    pub session_id: Option<Id>,
    pub conversation: Option<McpSessionBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedCursor<T> {
    pub version: u32,
    pub scope_binding: String,
    pub cursor: T,
}

/// Frozen query facts, not a reusable execution permission. A service must load
/// current authenticated grants for every new request and revalidate at action
/// boundaries. No writes, raw Store, or filesystem readers are exposed here.
pub struct WorkstreamRead {
    pub(super) store: Store,
    pub(super) project: Id,
    stream: Workstream,
    revision: Revision,
    binding: String,
}

pub(crate) fn visible(table: &str, id: &str) -> String {
    // Both fragments are static SQL identifiers supplied only by this crate.
    format!("EXISTS(SELECT 1 FROM temp.awr_read_visibility v WHERE v.kind='{table}' AND v.id={id})")
}

impl Store {
    /// Copy coherent facts before resolving selectors and authorization. Source
    /// refresh remains the caller's responsibility; stale authority is rejected.
    /// Only temporary membership and derived search indexes are changed.
    pub fn read_workstream(
        &self,
        project: Id,
        access: &WorkstreamAccess,
        selection: &WorkstreamReadSelection,
        max_snapshot_bytes: u64,
    ) -> Result<WorkstreamRead> {
        access.validate()?;
        if access.project_id != project.to_string() {
            return Err(WorkstreamError::AccessDenied.into());
        }
        let store = self.memory_snapshot(max_snapshot_bytes)?;
        let catalog = store.workstream_catalog(project)?;
        let work = selection
            .work_item_key
            .as_deref()
            .map(|key| {
                let work = store.work_item(project, key).map_err(private_lookup)?;
                store.workstream_binding(project, work.item.meta.id)
            })
            .transpose()?;
        if let Some(work) = &work {
            access.authorize(&catalog, work.workstream_id, WorkstreamAction::Read)?;
        }
        let session = selection
            .session_id
            .map(|id| {
                store.session(project, id).map_err(private_lookup)?;
                store
                    .session_workstream(project, id)
                    .map_err(private_lookup)
            })
            .transpose()?;
        if let Some(saved) = &session {
            let scope = saved.workstream_id.ok_or(WorkstreamError::AccessDenied)?;
            access.authorize(&catalog, scope, WorkstreamAction::Read)?;
            if work.as_ref().is_some_and(|work| {
                saved.work_item_id.as_deref() != Some(work.work_item_id.as_str())
                    || saved.workstream_id != Some(work.workstream_id)
            }) {
                return Err(WorkstreamError::BindingMismatch.into());
            }
        }
        let selected = session.as_ref().and_then(|s| s.workstream_id);
        if selection
            .workstream_id
            .zip(selected)
            .is_some_and(|(a, b)| a != b)
        {
            return Err(WorkstreamError::BindingMismatch.into());
        }
        let conversation_default = selection
            .conversation
            .as_ref()
            .map(|binding| store.conversation_workstream(project, binding))
            .transpose()?
            .flatten();
        let resolved = resolve_workstream(
            &catalog,
            access,
            &WorkstreamSelection {
                explicit: selection.workstream_id.or(selected),
                work,
                conversation_default,
                ..Default::default()
            },
            WorkstreamAction::Read,
        )?;
        let stream = catalog.get(resolved.workstream_id)?.clone();
        let revision = store.project(project)?.project_revision;
        let binding = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                "awr.scoped_read.v1",
                project,
                &access.subject,
                stream.id,
                stream.authority_version,
            ))?)
        );
        scope_membership::install(&store, project, &stream)?;
        Ok(WorkstreamRead {
            store,
            project,
            stream,
            revision,
            binding,
        })
    }
}

fn private_lookup(error: Error) -> Error {
    match error {
        Error::NotFound(_) | Error::EvidenceMissing(_) => WorkstreamError::AccessDenied.into(),
        error => error,
    }
}

impl WorkstreamRead {
    pub fn project_id(&self) -> Id {
        self.project
    }
    pub fn workstream(&self) -> &Workstream {
        &self.stream
    }
    /// An audit cursor; this is not a semantic context/cache identity.
    pub fn project_revision(&self) -> Revision {
        self.revision
    }

    pub(super) fn require(&self, kind: &str, id: Id) -> Result<()> {
        let allowed: bool = self
            .store
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM temp.awr_read_visibility WHERE kind=?1 AND id=?2)",
                params![kind, id.to_string()],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        if allowed {
            Ok(())
        } else {
            Err(WorkstreamError::AccessDenied.into())
        }
    }

    fn resolve(&self, table: &str, key: &str) -> Result<Id> {
        let sql = format!(
            "SELECT e.id FROM {table} e WHERE e.project_id=?1 AND (e.id=?2 OR e.external_key=?2) AND {} LIMIT 2",
            visible(table, "e.id")
        );
        let rows = self
            .store
            .conn
            .prepare(&sql)
            .map_err(db_error)?
            .query_map(params![self.project.to_string(), key], |row| {
                crate::catalog::id_at(row, 0)
            })
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)?;
        match rows.as_slice() {
            [id] => Ok(*id),
            [] => Err(WorkstreamError::AccessDenied.into()),
            _ => Err(Error::InvalidInput(
                "ambiguous visible object reference".into(),
            )),
        }
    }

    fn cursor<'a, T>(&self, cursor: Option<&'a ScopedCursor<T>>) -> Result<Option<&'a T>> {
        cursor
            .map(|c| {
                if c.version != 1 || c.scope_binding != self.binding {
                    Err(WorkstreamError::AccessDenied.into())
                } else {
                    Ok(&c.cursor)
                }
            })
            .transpose()
    }
    fn bind<T>(&self, cursor: Option<T>) -> Option<ScopedCursor<T>> {
        cursor.map(|cursor| ScopedCursor {
            version: 1,
            scope_binding: self.binding.clone(),
            cursor,
        })
    }

    pub fn work_item(&self, reference: &str) -> Result<Projected<WorkItem>> {
        let id = self.resolve("work_items", reference)?;
        self.store.work_item_by_id(self.project, id)
    }
    pub fn session(&self, id: Id) -> Result<Session> {
        self.require("sessions", id)?;
        self.store.session(self.project, id)
    }
    pub fn checkpoint(&self, id: Id) -> Result<Checkpoint> {
        self.require("checkpoints", id)?;
        self.store.checkpoint(self.project, id)
    }
    pub fn artifact(&self, id: Id) -> Result<Artifact> {
        self.require("artifacts", id)?;
        self.store.artifact(self.project, id)
    }
    pub fn evidence(&self, reference: &str) -> Result<EvidenceRecord> {
        let id = self.resolve("evidence", reference)?;
        self.store.evidence_by_id(self.project, id)
    }
    pub fn event(&self, id: Id) -> Result<Event> {
        self.require("events", id)?;
        self.store.event(self.project, id)
    }
    pub fn recovery_checkpoint(&self, session: Id) -> Result<Option<Checkpoint>> {
        self.require("sessions", session)?;
        let checkpoint = self.store.recovery_checkpoint(self.project, session)?;
        if let Some(cp) = &checkpoint {
            self.require("checkpoints", cp.id)?;
        }
        Ok(checkpoint)
    }
    pub fn resume_candidates(
        &self,
        work: Option<&str>,
        branch: Option<Id>,
    ) -> Result<Vec<Session>> {
        if let Some(key) = work {
            self.resolve("work_items", key)?;
        }
        self.store
            .resume_candidates_scoped(self.project, work, branch, true)
    }
    /// Counts, limits, and the cursor are computed within the same visibility set.
    /// The legacy cursor field is removed; use the separately returned scoped cursor.
    pub fn catalog_page(
        &self,
        kind: CatalogKind,
        state: CatalogScope,
        cursor: Option<&ScopedCursor<CatalogCursor>>,
        limit: usize,
    ) -> Result<(CatalogPage, Option<ScopedCursor<CatalogCursor>>)> {
        if kind == CatalogKind::Source {
            return Err(Error::Unsupported(
                "whole-source catalogs require project-level access".into(),
            ));
        }
        let cursor = self.cursor(cursor)?;
        if let Some(cursor) = cursor {
            self.require(kind.table(), cursor.after_id)?;
        }
        let mut page = self.store.catalog_page_scoped(
            self.project,
            self.revision,
            kind,
            state,
            cursor,
            limit,
            true,
        )?;
        let cursor = self.bind(page.next_cursor.take());
        Ok((page, cursor))
    }
    pub fn query_events(
        &self,
        query: &EventQuery,
        cursor: Option<&ScopedCursor<EventCursor>>,
    ) -> Result<(EventPage, Option<ScopedCursor<EventCursor>>)> {
        if query.cursor.is_some() {
            return Err(Error::InvalidInput(
                "scoped reads require a scope-bound event cursor".into(),
            ));
        }
        if let Some(id) = query.work_item_id {
            // Historical events may remain visible after the work moves away.
            let historical: bool = self.store.conn.query_row(
                &format!("SELECT EXISTS(SELECT 1 FROM events e WHERE e.project_id=?1 AND e.work_item_id=?2 AND {})",visible("events","e.id")),
                params![self.project.to_string(),id.to_string()], |row| row.get(0),
            ).map_err(db_error)?;
            if !historical {
                self.require("work_items", id)?;
            }
        }
        if let Some(id) = query.session_id {
            self.require("sessions", id)?;
        }
        if let Some(id) = query.source_id {
            self.require("sources", id)?;
        }
        if let crate::BranchFilter::Branch(id) = query.branch {
            self.require("branches", id)?;
        }
        let mut query = query.clone();
        query.cursor = self.cursor(cursor)?.cloned();
        if let Some(cursor) = &query.cursor {
            self.require("events", cursor.event_id)?;
        }
        let mut page = self.store.query_events_scoped(self.project, &query)?;
        let cursor = self.bind(page.next_cursor.take());
        Ok((page, cursor))
    }
    /// This snapshot's FTS corpus contains only visible objects. Other scopes
    /// cannot change BM25 ranking, truncation, or populate a shared search cache.
    pub fn search(&mut self, query: &SearchQuery) -> Result<SearchReport> {
        if let Some(key) = &query.work_item_key {
            self.resolve("work_items", key)?;
        }
        self.store.search_scoped(self.project, query, true)
    }
}
