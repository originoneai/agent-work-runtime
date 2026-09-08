use crate::{
    Store,
    catalog::{SOURCE_COLUMNS, id_at, revision_at, source_row},
    db_error,
    projection::table,
    session::session_at,
    transaction::{optional_id, sqlite_revision},
};
use awr_core::{
    EntityKind, Error, Event, EventDraft, Freshness, Id, MutationDraft, MutationPatch,
    MutationProposal, Projected, ProjectionMeta, ProposalAction, ProposalStatus, Result, Revision,
    Source,
};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::Value;

const PROPOSAL_COLUMNS: &str = "id,project_id,work_item_id,source_id,base_fingerprint,expected_revision,mutation_type,patch_json,status,created_by_session,revision";

fn proposal_status(row: &Row<'_>, column: usize) -> rusqlite::Result<ProposalStatus> {
    let raw: String = row.get(column)?;
    serde_json::from_value(Value::String(raw)).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn proposal_row(row: &Row<'_>) -> rusqlite::Result<MutationProposal> {
    let patch = serde_json::from_str(&row.get::<_, String>(7)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(MutationProposal {
        id: id_at(row, 0)?,
        project_id: id_at(row, 1)?,
        work_item_id: optional_id(row, 2)?,
        source_id: id_at(row, 3)?,
        base_fingerprint: row.get(4)?,
        expected_revision: revision_at(row, 5)?,
        mutation_type: row.get(6)?,
        patch,
        status: proposal_status(row, 8)?,
        created_by_session: optional_id(row, 9)?,
        revision: revision_at(row, 10)?,
    })
}

pub(crate) fn proposal_at(conn: &Connection, project: Id, id: Id) -> Result<MutationProposal> {
    conn.query_row(
        &format!("SELECT {PROPOSAL_COLUMNS} FROM mutation_proposals WHERE project_id=?1 AND id=?2"),
        params![project.to_string(), id.to_string()],
        proposal_row,
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| Error::NotFound(format!("mutation proposal {id}")))
}

fn mutation_target_at(
    conn: &Connection,
    project: Id,
    kind: EntityKind,
    reference: &str,
) -> Result<Projected<Value>> {
    if reference.trim().is_empty() {
        return Err(Error::InvalidInput(
            "mutation target reference must not be empty".into(),
        ));
    }
    let source_columns = SOURCE_COLUMNS
        .split(',')
        .map(|column| format!("source.{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {source_columns},target.payload_json,project.project_revision
         FROM {} target
         JOIN sources source ON source.project_id=target.project_id AND source.id=target.source_id AND source.active=1
         JOIN projects project ON project.id=target.project_id
         WHERE target.project_id=?1 AND target.active=1 AND (target.id=?2 OR target.external_key=?2)
         ORDER BY target.external_key,target.id LIMIT 2",
        table(kind)
    );
    let mut matches = conn
        .prepare(&sql)
        .map_err(db_error)?
        .query_map(params![project.to_string(), reference], |row| {
            let item = serde_json::from_str(&row.get::<_, String>(11)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(Projected {
                item,
                source: source_row(row)?,
                project_revision: revision_at(row, 12)?,
            })
        })
        .map_err(db_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(db_error)?;
    match matches.len() {
        0 => Err(Error::NotFound(format!(
            "source-backed {kind:?} mutation target {reference}"
        ))),
        1 => Ok(matches.pop().expect("one target match")),
        _ => Err(Error::InvalidInput(format!(
            "ambiguous {kind:?} mutation target {reference}"
        ))),
    }
}

fn source_at(conn: &Connection, project: Id, id: Id) -> Result<Source> {
    conn.query_row(
        &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE project_id=?1 AND id=?2 AND active=1"),
        params![project.to_string(), id.to_string()],
        source_row,
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| Error::NotFound(format!("active source {id}")))
}

pub(crate) fn bound_mutation_target_at(
    conn: &Connection,
    project: Id,
    patch: &MutationPatch,
    source_id: Id,
) -> Result<Projected<Value>> {
    let source_columns = SOURCE_COLUMNS
        .split(',')
        .map(|column| format!("source.{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {source_columns},target.payload_json,project.project_revision
         FROM {} target
         JOIN sources source ON source.project_id=target.project_id AND source.id=target.source_id AND source.active=1
         JOIN projects project ON project.id=target.project_id
         WHERE target.project_id=?1 AND target.active=1 AND target.id=?2
           AND target.external_key=?3 AND target.source_id=?4",
        table(patch.target.kind)
    );
    conn.query_row(
        &sql,
        params![
            project.to_string(),
            patch.target.meta.id.to_string(),
            patch.target.meta.external_key,
            source_id.to_string()
        ],
        |row| {
            let item = serde_json::from_str(&row.get::<_, String>(11)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(Projected {
                item,
                source: source_row(row)?,
                project_revision: revision_at(row, 12)?,
            })
        },
    )
    .optional()
    .map_err(db_error)?
    .ok_or_else(|| {
        Error::SourceConflict(format!(
            "mutation target {} is missing, retired, or rebound",
            patch.target.meta.id
        ))
    })
}

fn projection_meta(value: &Value) -> Result<ProjectionMeta> {
    serde_json::from_value(value.clone())
        .map_err(|error| Error::InvalidInput(format!("target lacks projection metadata: {error}")))
}

pub(crate) fn validate_binding(
    conn: &Connection,
    project: Id,
    source_id: Id,
    base_fingerprint: &str,
    mutation_type: &str,
    patch: &MutationPatch,
) -> Result<Projected<Value>> {
    patch.validate()?;
    if mutation_type != patch.mutation_type()
        || source_id != patch.target.meta.source_ref.source_id
        || base_fingerprint != patch.target.meta.source_ref.source_fingerprint
    {
        return Err(Error::InvalidInput(
            "proposal envelope disagrees with its immutable binding".into(),
        ));
    }
    let source = source_at(conn, project, source_id)?;
    match source.freshness {
        Freshness::Fresh => {}
        Freshness::Stale => {
            return Err(Error::SourceStale(format!("proposal source {source_id}")));
        }
        Freshness::Unavailable => {
            return Err(Error::SourceUnavailable(format!(
                "proposal source {source_id}"
            )));
        }
    }
    let expected_source = &patch.target.meta.source_ref;
    if source.fingerprint != base_fingerprint
        || source.fingerprint != expected_source.source_fingerprint
        || source.revision != expected_source.source_revision
        || source.config != patch.source_config
    {
        return Err(Error::SourceConflict(format!(
            "source {source_id} no longer matches the proposal binding"
        )));
    }
    let target = bound_mutation_target_at(conn, project, patch, source_id)?;
    let current_meta = projection_meta(&target.item)?;
    if target.source.id != source_id
        || serde_json::to_value(&current_meta)? != serde_json::to_value(&patch.target.meta)?
    {
        return Err(Error::SourceConflict(format!(
            "mutation target {} no longer matches its indexed revision and source reference",
            patch.target.meta.id
        )));
    }
    Ok(target)
}

fn target_work_id(patch: &MutationPatch, target: &Value) -> Result<Option<Id>> {
    match patch.target.kind {
        EntityKind::WorkItem => Ok(Some(patch.target.meta.id)),
        EntityKind::Evidence => {
            serde_json::from_value(target.get("work_item_id").cloned().unwrap_or(Value::Null))
                .map_err(|error| {
                    Error::InvalidInput(format!(
                        "evidence target has invalid work binding: {error}"
                    ))
                })
        }
        _ => Ok(None),
    }
}

fn proposal_work_and_session(
    conn: &Connection,
    project: Id,
    target_work: Option<Id>,
    created_by_session: Option<Id>,
) -> Result<(Option<Id>, Option<Id>)> {
    let Some(session_id) = created_by_session else {
        return Ok((target_work, None));
    };
    let session = session_at(conn, project, session_id)?;
    if session.status != "active" {
        return Err(Error::InvalidTransition(format!(
            "proposal session {session_id} is {}",
            session.status
        )));
    }
    if let Some(target_work) = target_work {
        if session
            .work_item_id
            .is_some_and(|session_work| session_work != target_work)
        {
            return Err(Error::InvalidInput(
                "work target conflicts with the creating session".into(),
            ));
        }
        Ok((Some(target_work), Some(session_id)))
    } else {
        Ok((session.work_item_id, Some(session_id)))
    }
}

pub(crate) fn proposal_branch(
    conn: &Connection,
    project: Id,
    proposal: &MutationProposal,
) -> Result<Option<Id>> {
    proposal
        .created_by_session
        .map(|session| session_at(conn, project, session).map(|session| session.branch_id))
        .transpose()
        .map(Option::flatten)
}

fn event_metadata(
    proposal: &MutationProposal,
    action: &str,
    from: Option<ProposalStatus>,
    to: ProposalStatus,
    actor: Option<&str>,
    reason: Option<&str>,
) -> Value {
    let target = proposal.patch.get("target");
    let intent = proposal
        .patch
        .get("intent")
        .and_then(Value::as_str)
        .filter(|intent| intent.len() <= 4096);
    serde_json::json!({
        "proposal_id": proposal.id,
        "source_id": proposal.source_id,
        "action": action,
        "from": from,
        "to": to,
        "actor": actor,
        "reason": reason,
        "intent": intent,
        "target_kind": target.and_then(|target| target.get("kind")),
        "target_id": target.and_then(|target| target.get("meta")).and_then(|meta| meta.get("id")),
        "target_key": target.and_then(|target| target.get("meta")).and_then(|meta| meta.get("external_key")),
        "proposal_revision": proposal.revision,
        "expected_revision": proposal.expected_revision,
        "work_action": proposal.patch.get("work_action"),
    })
}

impl Store {
    /// Resolve one active source projection by exact ULID or external key.
    pub fn mutation_target(
        &self,
        project: Id,
        kind: EntityKind,
        reference: &str,
    ) -> Result<Projected<Value>> {
        mutation_target_at(&self.conn, project, kind, reference)
    }

    pub fn create_proposal(
        &mut self,
        project: Id,
        expected: Revision,
        draft: MutationDraft,
    ) -> Result<(MutationProposal, Event)> {
        draft.patch.validate()?;
        if draft.mutation_type != draft.patch.mutation_type() {
            return Err(Error::InvalidInput(
                "mutation type disagrees with its validated proposal envelope".into(),
            ));
        }
        let id = Id::new();
        let patch_value = serde_json::to_value(&draft.patch)?;
        self.runtime_transaction_with_event(
            project,
            expected,
            EventDraft::new("proposal.created", "Created source mutation proposal"),
            move |tx, _, event| {
                let target = validate_binding(
                    tx,
                    project,
                    draft.source_id,
                    &draft.base_fingerprint,
                    &draft.mutation_type,
                    &draft.patch,
                )?;
                crate::work_action::validate_action(tx,project,expected,&draft.patch,&target.item,draft.created_by_session)?;
                let target_work = target_work_id(&draft.patch, &target.item)?;
                let (work_item_id, session_id) =
                    proposal_work_and_session(tx, project, target_work, draft.created_by_session)?;
                let proposal = MutationProposal {
                    id,
                    project_id: project,
                    work_item_id,
                    source_id: draft.source_id,
                    base_fingerprint: draft.base_fingerprint,
                    expected_revision: expected,
                    mutation_type: draft.mutation_type,
                    patch: patch_value,
                    status: ProposalStatus::Draft,
                    created_by_session: draft.created_by_session,
                    revision: 1,
                };
                tx.execute(
                    "INSERT INTO mutation_proposals(id,project_id,work_item_id,source_id,base_fingerprint,expected_revision,mutation_type,patch_json,status,created_by_session,revision,created_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'draft',?9,1,?10)",
                    params![
                        proposal.id.to_string(),
                        project.to_string(),
                        proposal.work_item_id.map(|work| work.to_string()),
                        proposal.source_id.to_string(),
                        proposal.base_fingerprint,
                        sqlite_revision(expected)?,
                        proposal.mutation_type,
                        serde_json::to_string(&proposal.patch)?,
                        proposal.created_by_session.map(|session| session.to_string()),
                        awr_core::now_millis()?,
                    ],
                )
                .map_err(db_error)?;
                event.work_item_id = proposal.work_item_id;
                event.session_id = session_id;
                event.payload = event_metadata(
                    &proposal,
                    "create",
                    None,
                    ProposalStatus::Draft,
                    None,
                    None,
                );
                Ok(proposal)
            },
        )
    }

    pub fn proposal(&self, project: Id, id: Id) -> Result<MutationProposal> {
        proposal_at(&self.conn, project, id)
    }

    pub fn proposals(
        &self,
        project: Id,
        status: Option<ProposalStatus>,
        limit: usize,
    ) -> Result<Vec<MutationProposal>> {
        if !(1..=100).contains(&limit) {
            return Err(Error::InvalidInput("proposal limit must be 1..100".into()));
        }
        self.project(project)?;
        let status = status.map(ProposalStatus::as_str);
        self.conn
            .prepare(&format!(
                "SELECT {PROPOSAL_COLUMNS} FROM mutation_proposals
                 WHERE project_id=?1 AND (?2 IS NULL OR status=?2)
                 ORDER BY revision DESC,id DESC LIMIT ?3"
            ))
            .map_err(db_error)?
            .query_map(
                params![project.to_string(), status, limit as i64],
                proposal_row,
            )
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)
    }

    pub fn review_proposal(
        &mut self,
        project: Id,
        expected: Revision,
        id: Id,
        action: ProposalAction,
        actor: &str,
        reason: &str,
    ) -> Result<(MutationProposal, Event)> {
        let actor = actor.trim();
        let reason = reason.trim();
        if actor.is_empty() || actor.len() > 256 || reason.is_empty() || reason.len() > 16_384 {
            return Err(Error::InvalidInput(
                "proposal review requires an actor of 1..256 bytes and reason of 1..16384 bytes"
                    .into(),
            ));
        }
        self.runtime_transaction_with_event(
            project,
            expected,
            EventDraft::new(action.event_type(), "Reviewed source mutation proposal"),
            |tx, _, event| {
                let mut proposal = proposal_at(tx, project, id)?;
                if crate::mutation_apply::pending_for_source(tx, project, proposal.source_id)?.is_some_and(|attempt| attempt.proposal_id == id) {
                    return Err(Error::InvalidTransition("proposal has an unfinished application; inspect and recover it before review".into()));
                }
                let from = proposal.status;
                let to = action.next_status(from)?;
                if matches!(
                    action,
                    ProposalAction::Submit
                        | ProposalAction::Approve
                        | ProposalAction::RequireManualApply
                ) {
                    let patch = proposal.bound_patch()?;
                    let target=validate_binding(
                        tx,
                        project,
                        proposal.source_id,
                        &proposal.base_fingerprint,
                        &proposal.mutation_type,
                        &patch,
                    )?;
                    crate::work_action::validate_action(tx,project,expected,&patch,&target.item,proposal.created_by_session)?;
                }
                let updated = tx
                    .execute(
                        "UPDATE mutation_proposals SET status=?1,revision=revision+1
                         WHERE project_id=?2 AND id=?3 AND status=?4 AND revision=?5",
                        params![
                            to.as_str(),
                            project.to_string(),
                            id.to_string(),
                            from.as_str(),
                            sqlite_revision(proposal.revision)?,
                        ],
                    )
                    .map_err(db_error)?;
                if updated != 1 {
                    return Err(Error::MutationConflict(format!(
                        "proposal {id} changed during review"
                    )));
                }
                proposal.status = to;
                proposal.revision = proposal
                    .revision
                    .checked_add(1)
                    .ok_or_else(|| Error::InvalidInput("proposal revision overflow".into()))?;
                event.work_item_id = proposal.work_item_id;
                event.branch_id = proposal_branch(tx, project, &proposal)?;
                event.payload = event_metadata(
                    &proposal,
                    serde_json::to_value(action)?.as_str().unwrap_or("review"),
                    Some(from),
                    to,
                    Some(actor),
                    Some(reason),
                );
                Ok(proposal)
            },
        )
    }
}
