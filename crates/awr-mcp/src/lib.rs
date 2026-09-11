//! MCP tools over project-isolated AWR domain services. No shell commands are executed.
pub use awr_core::{Error, Result};
mod arguments;
pub mod hub;
mod lifecycle;
mod operations;
mod project;
mod schema;

use hub::{Hub, Principal, ProjectService};
use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};
pub use schema::{TOOL_NAMES, tools};
use std::{path::Path, sync::Arc};

#[derive(Clone)]
pub struct AwrServer {
    project: Option<Arc<ProjectService>>,
    hub: Option<Arc<Hub>>,
    capacity: Arc<tokio::sync::Semaphore>,
}
impl AwrServer {
    /// Bind one project directory. Each operation validates its database/source state;
    /// status can explain how to initialize or repair a project before a database exists.
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            project: Some(Arc::new(ProjectService::open(root, None)?)),
            hub: None,
            capacity: Arc::new(tokio::sync::Semaphore::new(64)),
        })
    }
    pub(crate) fn shared(hub: Arc<Hub>) -> Self {
        Self {
            project: None,
            capacity: hub.capacity.clone(),
            hub: Some(hub),
        }
    }
    fn catalog(&self) -> Vec<Tool> {
        if self.hub.is_some() {
            schema::shared_tools()
        } else {
            tools()
        }
    }
}

impl ServerHandler for AwrServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("awr-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions("AWR is source-first. When awr_projects_list is available, discover registered keys and pass project on every project call. Start with awr_project_status and follow organization.actions; an empty ledger or intake draft is not business readiness. Read tools never persist changes. On SourceStale, explicitly reindex sources and inspect again. Use awr_session_start with a stable host conversation and claim before source transitions. Select your session or conversation on subsequent calls; HTTP connections never select or end work sessions. Mutations require a reviewed expected_revision; completion also requires bound evidence. Command/report metadata is never executed. Inspect durable receipts before retrying an interrupted mutation.")
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|r| r.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "this tool catalog has no next cursor",
                None,
            ));
        }
        let mut result = ListToolsResult::default();
        result.tools = self.catalog();
        Ok(result)
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.catalog().into_iter().find(|t| t.name == name)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown AWR tool",
                None,
            ));
        }
        let mut args = request.arguments.unwrap_or_default();
        let principal = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Principal>())
            .cloned();
        let selected = if let Some(hub) = &self.hub {
            let principal = principal
                .as_ref()
                .ok_or_else(|| ErrorData::invalid_params("authenticated client required", None))?;
            if request.name == "awr_projects_list" {
                if !args.is_empty() {
                    return Err(ErrorData::invalid_params(
                        "project catalog takes no arguments",
                        None,
                    ));
                }
                return Ok(CallToolResult::structured(hub.catalog(&principal)).into());
            }
            let selected = args
                .remove("project")
                .and_then(|v| v.as_str().map(str::to_owned));
            match selected {
                Some(key) => hub.select(&principal, &key, !operations::is_read_only(&request.name)),
                None => Err(Error::InvalidInput(
                    "shared MCP calls require an explicit project key".into(),
                )),
            }
        } else {
            Ok(self.project.as_ref().expect("bound project").clone())
        };
        let project = match selected {
            Ok(project) => project,
            Err(error) => {
                return Ok(CallToolResult::structured_error(
                    serde_json::to_value(error.report()).unwrap(),
                )
                .into());
            }
        };
        let permit = self.capacity.clone().try_acquire_owned().map_err(|_| {
            ErrorData::internal_error(
                "MCP service busy; retry after inspecting any prior write receipt",
                None,
            )
        })?;
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            project.call(
                &request.name,
                args,
                principal.as_ref().map(|p| p.id.as_str()),
            )
        })
        .await
        .map_err(|e| ErrorData::internal_error(format!("MCP domain worker failed: {e}"), None))?;
        Ok(match result {
            Ok(result) => result,
            Err(error) => CallToolResult::structured_error(
                serde_json::to_value(error.report()).expect("error report serializes"),
            ),
        }
        .into())
    }
}
