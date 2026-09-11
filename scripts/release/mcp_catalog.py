"""Required installed stdio API, shared by package smoke and assembly checks."""

STDIO_TOOLS = frozenset({
    "awr_project_status", "awr_work_ready", "awr_work_get", "awr_context_compile",
    "awr_work_transition", "awr_event_append", "awr_evidence_record", "awr_search",
    "awr_session_start", "awr_session_get", "awr_session_list",
    "awr_session_checkpoint", "awr_session_end", "awr_session_resume", "awr_session_claim",
    "awr_session_wait", "awr_session_reply", "awr_operation_get",
    "awr_operation_recover", "awr_source_reindex",
})


def validate_stdio_tools(names):
    assert isinstance(names, list) and all(isinstance(name, str) for name in names), "invalid MCP tool names"
    actual = set(names)
    assert len(actual) == len(names), "duplicate MCP tool names"
    assert actual == STDIO_TOOLS, (
        f"MCP catalog mismatch: missing={sorted(STDIO_TOOLS - actual)}, "
        f"unexpected={sorted(actual - STDIO_TOOLS)}"
    )
    return sorted(actual)
