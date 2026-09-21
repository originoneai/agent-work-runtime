//! Read-only authenticated workstream boundary for the shared personal service.
//! Local stdio is an owner integration, not a server ACL or a filesystem sandbox.
use crate::{
    operations::parse,
    project::{ReadProject, database},
};
use awr_context::{ContextRequest, DeltaBaseline, compile_workstream_context};
use awr_core::*;
use awr_source::Manifest;
use awr_store::{
    CatalogCursor, CatalogKind, CatalogScope, EventCursor, EventQuery, ScopedCursor, SearchQuery,
    Store, WorkstreamReadSelection,
};
use rmcp::model::{CallToolResult, JsonObject, Tool, ToolAnnotations};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

const ACTIONS: &[&str] = &[
    "capabilities",
    "list",
    "context",
    "catalog",
    "search",
    "object",
    "events",
    "recovery",
];

pub(crate) fn tool() -> Tool {
    let schema = json!({"type":"object","additionalProperties":false,
        "required":["project","protocol_version","action"],"properties":{
            "project":{"type":"string"},"protocol_version":{"type":"integer","const":1},
            "action":{"type":"string","enum":ACTIONS},
            "workstream":{"type":"string"},"work":{"type":"string"},"session":{"type":"string"},
            "args":{"type":"object","description":"Action-specific fields; use capabilities for schemas."}}});
    let mut tool = Tool::new(
        "awr_workstream",
        "Shared-service scoped reads. Start with capabilities, then list authorized scopes or bind work/session. Unsupported writes are rejected. Grants come from the operator, never from tool arguments.",
        schema.as_object().unwrap().clone(),
    );
    tool.annotations = Some(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    );
    tool
}

/// Check both the retained authority and the current manifest: pending enablement
/// must not fall back to an old project-wide tool. Invalid boundaries fail closed.
pub(crate) fn enabled(root: &Path, project: Id) -> Result<bool> {
    let check = || {
        let store = Store::open_readonly(&database(root)?)?;
        let retained = store.workstreams_enabled(project)?;
        let manifest = Manifest::load(root)?;
        Ok(retained
            || manifest
                .sources
                .iter()
                .any(|s| s.adapter == "yaml-workstream-ledger-v1"))
    };
    check().map_err(boundary_error)
}

fn boundary_error(_: Error) -> Error {
    Error::SourceStale("workstream boundary cannot be verified; the project operator must check sources and explicitly reindex".into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    protocol_version: u32,
    action: String,
    workstream: Option<Id>,
    work: Option<String>,
    session: Option<Id>,
    #[serde(default)]
    args: JsonObject,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct ContextOptions {
    budget: Option<usize>,
    intent: Option<String>,
    checkpoint: Option<Id>,
    after_revision: Option<Revision>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogOptions {
    kind: CatalogKind,
    #[serde(default = "active")]
    state: CatalogScope,
    cursor: Option<ScopedCursor<CatalogCursor>>,
    #[serde(default = "ten")]
    limit: usize,
}
fn active() -> CatalogScope {
    CatalogScope::Active
}
fn ten() -> usize {
    10
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObjectOptions {
    kind: String,
    reference: String,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct EventOptions {
    after_revision: Revision,
    cursor: Option<ScopedCursor<EventCursor>>,
    limit: Option<usize>,
}

fn capabilities() -> Value {
    json!({"protocol":"awr-shared-workstream","protocol_version":1,
        "reads":ACTIONS,"writes":[],"artifact_content":false,"team_postgres":false,
        "filesystem_sandbox":false,"cross_workstream_deliveries":false,
        "legacy_tools":"rejected for workstream-enabled projects or scoped principals",
        "selection":"work/session binding, explicit workstream, or unique authorized workstream; selectors must agree",
        "recheck":"each request verifies source authority and operator-pinned grant version; operator credential/grant changes require a service restart",
        "arguments":{
            "capabilities":{},"list":{},
            "context":{"budget":"optional 1..100000, default 5000","intent":"optional string","checkpoint":"optional ID","after_revision":"optional integer; exclusive with checkpoint"},
            "catalog":{"kind":"goal|plan|rule|work|decision|relation|artifact|evidence","state":"active|retired|all, default active","limit":"1..100, default 10","cursor":"scope-bound next_cursor, optional"},
            "search":{"text":"optional string","kind":"optional kind","status":"optional state","work":"optional work key","limit":"1..100, default 10"},
            "object":{"kind":"work|session|checkpoint|artifact|evidence|event","reference":"key or ID; artifact returns metadata only"},
            "events":{"after_revision":"optional integer","limit":"1..100, default 10","cursor":"scope-bound next_cursor, optional"},
            "recovery":{}}})
}

pub(crate) fn call(
    root: &Path,
    project: Id,
    access: Option<&WorkstreamAccess>,
    args: JsonObject,
) -> Result<CallToolResult> {
    let request: Request = parse(Value::Object(args))?;
    if request.protocol_version != 1 || !ACTIONS.contains(&request.action.as_str()) {
        return Err(Error::Unsupported(
            "unsupported shared workstream protocol or action".into(),
        ));
    }
    if matches!(request.action.as_str(), "capabilities" | "list")
        && (request.workstream.is_some()
            || request.work.is_some()
            || request.session.is_some()
            || !request.args.is_empty())
    {
        return Err(Error::InvalidInput(
            "capabilities/list take no selectors or action arguments".into(),
        ));
    }
    if request.action == "capabilities" {
        return Ok(CallToolResult::structured(capabilities()));
    }
    let access = access.ok_or(WorkstreamError::AccessDenied)?;
    let mut view = ReadProject::open(root).map_err(boundary_error)?;
    if view.project.id != project || access.project_id != project.to_string() {
        return Err(WorkstreamError::AccessDenied.into());
    }
    if !view
        .store
        .workstreams_enabled(project)
        .map_err(boundary_error)?
    {
        return Err(Error::Unsupported(
            "workstreams must be explicitly enabled by the project operator".into(),
        ));
    }
    if request.action == "list" {
        let catalog = view
            .store
            .workstream_catalog(project)
            .map_err(boundary_error)?;
        let mut streams = Vec::new();
        for grant in &access.grants {
            access.authorize(&catalog, grant.workstream_id, WorkstreamAction::Read)?;
            streams.push(catalog.get(grant.workstream_id)?.clone());
        }
        view.finish(root).map_err(boundary_error)?;
        return Ok(CallToolResult::structured(
            json!({"ok":true,"read_only":true,"workstreams":streams}),
        ));
    }
    let selection = WorkstreamReadSelection {
        workstream_id: request.workstream,
        work_item_key: request.work,
        session_id: request.session,
        conversation: None,
    };
    let mut read = view
        .store
        .read_workstream(project, access, &selection, 256 * 1024 * 1024)?;
    let args = Value::Object(request.args);
    let result = match request.action.as_str() {
        "context" => {
            let options: ContextOptions = parse(args)?;
            if options.checkpoint.is_some() && options.after_revision.is_some() {
                return Err(Error::InvalidInput(
                    "use checkpoint or after_revision, not both".into(),
                ));
            }
            let context = ContextRequest {
                work_item_key: selection.work_item_key.clone(),
                session_id: selection.session_id,
                token_budget: options.budget.unwrap_or(5000),
                intent: options.intent.unwrap_or_else(|| "work".into()),
                delta_baseline: if let Some(id) = options.checkpoint {
                    DeltaBaseline::Checkpoint { id }
                } else if let Some(revision) = options.after_revision {
                    DeltaBaseline::Revision { revision }
                } else {
                    DeltaBaseline::Auto
                },
                ..Default::default()
            };
            serde_json::to_value(compile_workstream_context(
                &mut view.store,
                root,
                &context,
                access,
                &selection,
            )?)?
        }
        "catalog" => {
            let options: CatalogOptions = parse(args)?;
            bounded(options.limit)?;
            let (page, next_cursor) = read.catalog_page(
                options.kind,
                options.state,
                options.cursor.as_ref(),
                options.limit,
            )?;
            json!({"page":page,"next_cursor":next_cursor})
        }
        "search" => {
            let options: crate::arguments::SearchArgs = parse(args)?;
            bounded(options.limit)?;
            serde_json::to_value(read.search(&SearchQuery {
                text: options.text,
                kind: options.kind.map(|kind| {
                    if kind == "work" {
                        "work_item".into()
                    } else {
                        kind
                    }
                }),
                status: options.status,
                work_item_key: options.work,
                limit: options.limit,
            })?)?
        }
        "object" => {
            let options: ObjectOptions = parse(args)?;
            let id = || {
                options
                    .reference
                    .parse::<Id>()
                    .map_err(|_| Error::InvalidInput("this object requires an ID".into()))
            };
            match options.kind.as_str() {
                "work" => serde_json::to_value(read.work_item(&options.reference)?)?,
                "session" => serde_json::to_value(read.session(id()?)?)?,
                "checkpoint" => serde_json::to_value(read.checkpoint(id()?)?)?,
                "artifact" => serde_json::to_value(read.artifact(id()?)?)?,
                "evidence" => serde_json::to_value(read.evidence(&options.reference)?)?,
                "event" => serde_json::to_value(read.event(id()?)?)?,
                _ => return Err(Error::Unsupported("unsupported scoped object kind".into())),
            }
        }
        "events" => {
            let options: EventOptions = parse(args)?;
            let limit = options.limit.unwrap_or(10);
            bounded(limit)?;
            let (page, next_cursor) = read.query_events(
                &EventQuery {
                    after_revision: options.after_revision,
                    limit,
                    ..Default::default()
                },
                options.cursor.as_ref(),
            )?;
            json!({"page":page,"next_cursor":next_cursor})
        }
        "recovery" => {
            if args != json!({}) {
                return Err(Error::InvalidInput("recovery takes selectors only".into()));
            }
            if let Some(session) = selection.session_id {
                json!({"session":read.session(session)?,"checkpoint":read.recovery_checkpoint(session)?})
            } else {
                json!({"candidates":read.resume_candidates(selection.work_item_key.as_deref(), None)?})
            }
        }
        _ => unreachable!("validated action"),
    };
    view.finish(root).map_err(boundary_error)?;
    // The audit cursor is not a workstream semantic identity or a write grant.
    let incomplete = request.action == "context" && result["completeness"]["complete"] == false;
    let value = json!({"ok":!incomplete,"read_only":true,"protocol_version":1,
        "workstream":read.workstream(),"scope_binding":read.scope_binding(),
        "project_revision":read.project_revision(),"result":result});
    ensure_public_value(&value)?;
    Ok(if incomplete {
        CallToolResult::structured_error(value)
    } else {
        CallToolResult::structured(value)
    })
}

fn bounded(limit: usize) -> Result<()> {
    if !(1..=100).contains(&limit) {
        return Err(Error::InvalidInput("limit must be 1..100".into()));
    }
    Ok(())
}
