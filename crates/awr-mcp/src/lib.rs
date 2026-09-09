//! Eight MCP tools over the existing AWR domain services. No shell commands are executed.
pub use awr_core::{Error, Result};
mod arguments;
mod operations;
mod project;
mod schema;

use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};
pub use schema::{TOOL_NAMES, tools};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct AwrServer {
    root: Arc<PathBuf>,
    // Serialize this server's domain operations. Other processes use domain CAS.
    operation: Arc<Mutex<()>>,
}
impl AwrServer {
    /// Bind one project directory. Each operation validates its database/source state;
    /// status can explain how to initialize or repair a project before a database exists.
    pub fn open(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(Error::InvalidInput(
                "MCP project root must be a directory".into(),
            ));
        }
        Ok(Self {
            root: Arc::new(root),
            operation: Arc::new(Mutex::new(())),
        })
    }
}

impl ServerHandler for AwrServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("awr-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions("AWR is source-first. Start with awr_project_status and follow organization.actions; an empty ledger or an intake draft is not business readiness. AWR diagnoses structure; the Coding Agent organizes source-backed goals and work and keeps uncertain intent explicit. Read tools never persist changes. On SourceStale, run awr source reindex explicitly and inspect again. Use awr session start --claim before source transitions. Mutations require a reviewed expected_revision; completion also requires bound evidence. Command/report metadata is never executed. Inspect durable receipts before retrying an interrupted mutation.")
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        if request.is_some_and(|r| r.cursor.is_some()) {
            return Err(ErrorData::invalid_params(
                "this eight-tool catalog has no next cursor",
                None,
            ));
        }
        let mut result = ListToolsResult::default();
        result.tools = tools();
        Ok(result)
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|t| t.name == name)
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if !TOOL_NAMES.contains(&request.name.as_ref()) {
            return Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                "unknown AWR tool",
                None,
            ));
        }
        let root = self.root.clone();
        let operation = self.operation.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _guard = operation.lock().map_err(|_| {
                Error::Storage("MCP operation lock is poisoned; restart the server".into())
            })?;
            operations::call(&root, &request.name, request.arguments.unwrap_or_default())
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
