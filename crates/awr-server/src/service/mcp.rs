//! Stateless MCP transport over the same transactional Team operations as HTTP.
use super::*;
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    middleware::{self, Next},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::*,
    service::RequestContext,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use tokio::{sync::OwnedSemaphorePermit, time::Instant};

#[derive(Clone)]
struct Endpoint {
    state: Arc<StateData>,
    project: ProjectBinding,
}

// Request-local only. Retaining the permit also bounds SDK handlers that outlive
// a disconnected HTTP receiver. No authenticated authority is cached here.
#[derive(Clone)]
struct RequestAccess {
    bearer: String,
    _permit: Arc<OwnedSemaphorePermit>,
    deadline: Instant,
}

pub(super) fn router(state: Arc<StateData>, project: ProjectBinding) -> Router {
    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true);
    config.allowed_hosts = state.hosts.clone();
    config.max_request_body_bytes = 65536;
    let path = format!("/v1/projects/{}/mcp", project.key);
    let endpoint = Endpoint { state, project };
    let handler = endpoint.clone();
    let service: StreamableHttpService<Endpoint, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(handler.clone()), Default::default(), config);
    Router::new()
        .nest_service(&path, service)
        .layer(middleware::from_fn_with_state(endpoint, authorize))
}

async fn authorize(State(endpoint): State<Endpoint>, request: Request, next: Next) -> Response {
    if !allowed_request(&endpoint.state, request.headers()) {
        return denied();
    }
    let Some(token) = bearer(request.headers()).map(str::to_owned) else {
        return denied();
    };
    let Ok(permit) = endpoint.state.permits.clone().try_acquire_owned() else {
        return response(StatusCode::SERVICE_UNAVAILABLE, json!({"code":"Busy"}));
    };
    let access = RequestAccess {
        bearer: token,
        _permit: Arc::new(permit),
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let result = tokio::time::timeout_at(access.deadline, async {
        let (mut parts, body) = request.into_parts();
        let Ok(body) = to_bytes(body, 65536).await else {
            return response(
                StatusCode::PAYLOAD_TOO_LARGE,
                json!({"code":"RequestTooLarge"}),
            );
        };
        // Initialize, discovery, ping and notifications also require current
        // access. Each actual tool then rechecks it in its own operation's tx.
        let capabilities: WorkstreamQuery =
            serde_json::from_value(json!({"protocol_version":1,"op":"capabilities"}))
                .expect("static capabilities query");
        if let Err(error) = endpoint
            .state
            .store
            .query(
                &endpoint.project.tenant_id,
                &endpoint.project.project_id,
                &access.bearer,
                capabilities,
            )
            .await
        {
            return error_response(error);
        }
        parts.extensions.insert(access.clone());
        let result = next.run(Request::from_parts(parts, Body::from(body))).await;
        let (mut parts, body) = result.into_parts();
        // Bound the actual MCP envelope, including the SDK's text fallback.
        let Ok(body) = to_bytes(body, 1_048_576).await else {
            return error_response(PgError::ResponseTooLarge);
        };
        parts
            .headers
            .insert("cache-control", "no-store".parse().unwrap());
        Response::from_parts(parts, Body::from(body))
    })
    .await;
    result.unwrap_or_else(|_| unavailable())
}

fn catalog() -> Vec<Tool> {
    let query = json!({"type":"object","additionalProperties":false,
    "required":["protocol_version","op"],"properties":{
        "protocol_version":{"type":"integer","const":1},
        "op":{"type":"string","enum":WorkstreamQuery::OPERATIONS},
        "workstream_id":{"type":"string"},"work_id":{"type":"string"},
        "session_id":{"type":"string"},"request_id":{"type":"string"},"claim_id":{"type":"string"},
        "execution_id":{"type":"string"},
        "search":{"type":"string","maxLength":512},"cursor":{"type":"string","maxLength":4096},
        "limit":{"type":"integer","minimum":1,"maximum":100},
        "max_context_bytes":{"type":"integer","minimum":1,"maximum":262144}
    }});
    let command = json!({"type":"object","additionalProperties":false,
    "required":["protocol_version","request_id","op","workstream_id","work_id","coordinator_epoch","expected_project_revision","expected_authority_version","expected_ownership_version","expected_contract_hash","args"],
    "properties":{
        "protocol_version":{"type":"integer","const":1},
        "op":{"type":"string","enum":WorkstreamCommand::OPERATIONS},
        "request_id":{"type":"string","maxLength":128},
        "workstream_id":{"type":"string"},"work_id":{"type":"string"},
        "coordinator_epoch":{"type":"string"},
        "expected_project_revision":{"type":"string","pattern":"^(0|[1-9][0-9]*)$"},
        "expected_authority_version":{"type":"string","pattern":"^[1-9][0-9]*$"},
        "expected_ownership_version":{"type":"string","pattern":"^[1-9][0-9]*$"},
        "expected_contract_hash":{"type":"string"},
        "args":{"type":"object","description":"session.start: conversation_id. session.checkpoint: session_id, expected_session_version, context_hash, next_action, open_loops. session.end: session_id, expected_session_version. All claim/execution actions: session_id, expected_session_version. claim.acquire adds expected_work_version (0 when runtime absent), ttl_seconds (1..3600). claim.renew/release add claim_id, expected_fence, expected_lease_version; renew also ttl_seconds. execution.prepare adds claim_id, expected_fence, expected_lease_version, expected_work_version, input_digest (64 lowercase hex), declared_scope (canonical relative paths). execution.cancel adds execution_id, expected_execution_version. execution.start adds execution_id, expected_execution_version, claim_id, expected_fence, expected_lease_version, expected_work_version, execution_mode (caller_managed or reference_write_v1), optional expected_input_digest. reference_write_v1 requires the prepared input digest and system attestation authority; the service does not dispatch the local runner. execution.report adds execution_id, expected_execution_version, outcome (succeeded/failed/cancelled/unknown), optional output_digest (required for success), observed_paths, note. execution.attest adds execution_id, expected_execution_version, facts. execution.reconcile also adds expected_work_version, reviewed_receipt_id (latest inspected ID or null), clear_recovery_block. facts: outcome, input_digest, optional output_digest (required for success), environment_digest, observed_paths, note. Digests are 64 lowercase hex. Versions are decimal strings; unknown fields fail."}
    }});
    vec![
        Tool::new("awr_team_query",
            "Scoped Team reads. Begin with capabilities, then workstreams.list or work.prepare. The endpoint binds the project; bearer grants bind the client. Re-prepare after relevant changes. No execution admission.",
            query.as_object().unwrap().clone())
            .with_annotations(ToolAnnotations::new().read_only(true).destructive(false).idempotent(true).open_world(false)),
        Tool::new("awr_team_command",
            "Durable sessions, claims and caller-managed execution. Use work.prepare preconditions and a stable request_id. Only a fresh execution.start response with execution_authorized=true permits one run under the live lease. Preparation, inspection and replay grant no execution rights. On unknown outcome inspect command.inspect before an exact retry; never repeat effects from a receipt. Refresh after conflicts or lease/contract changes. Cancellation is a request after start. Reports remain caller_asserted. Attestation requires operator-issued system authority at admission and now. For unknown effects, execution.inspect then operator execution.reconcile; confirm current versions and latest receipt. Recheck on permission, receipt or work changes. Settlement is not work completion.",
            command.as_object().unwrap().clone())
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(false).idempotent(true).open_world(false)),
    ]
}

impl ServerHandler for Endpoint {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("awr-team-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions("The URL binds one operator-registered project; bearer credentials are checked on every request. Begin with awr_team_query capabilities. Work/session selectors bind a workstream; missing permissions never mean satisfied dependencies. Consume work.prepare before checkpointing. Session journals and claims grant no execution rights. Claim replay is a historical receipt; use claim.inspect for current lease state. If a command outcome is unknown, inspect its original request_id before an exact retry. Recheck context and permission after relevant changes. MCP connection closure never closes a durable work session.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        catalog().into_iter().find(|tool| tool.name == name)
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|r| r.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "tool catalog has no next cursor",
                None,
            ));
        }
        let mut result = ListToolsResult::default();
        result.tools = catalog();
        Ok(result)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let access = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|p| p.extensions.get::<RequestAccess>())
            .cloned()
            .ok_or_else(|| ErrorData::invalid_params("authenticated request required", None))?;
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = tokio::time::timeout_at(access.deadline, async {
            match request.name.as_ref() {
                "awr_team_query" => {
                    let q: WorkstreamQuery = serde_json::from_value(args)
                        .map_err(|_| PgError::Protocol("invalid query".into()))?;
                    self.state
                        .store
                        .query(
                            &self.project.tenant_id,
                            &self.project.project_id,
                            &access.bearer,
                            q,
                        )
                        .await
                }
                "awr_team_command" => {
                    let c: WorkstreamCommand = serde_json::from_value(args)
                        .map_err(|_| PgError::Protocol("invalid command".into()))?;
                    self.state
                        .commands
                        .execute(
                            &self.project.tenant_id,
                            &self.project.project_id,
                            &access.bearer,
                            c,
                        )
                        .await
                }
                _ => Err(PgError::Unsupported("tool unavailable".into())),
            }
        })
        .await;
        let result = match result {
            Ok(Ok(value)) => CallToolResult::structured(value),
            Ok(Err(error)) => CallToolResult::structured_error(public_error(error).1),
            Err(_) => CallToolResult::structured_error(unavailable_value()),
        };
        Ok(result.into())
    }
}
