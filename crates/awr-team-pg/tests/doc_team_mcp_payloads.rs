//! TMCP-041: documented Team MCP examples must deserialize as production types.
use awr_team_pg::{WorkstreamCommand, WorkstreamQuery};

fn json_fences(doc: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = doc;
    while let Some(start) = rest.find("```json") {
        rest = &rest[start + 7..];
        let body = rest.strip_prefix('\n').unwrap_or(rest);
        let Some(end) = body.find("```") else { break };
        let block = body[..end].trim();
        if !block.is_empty() {
            out.push(block.to_string());
        }
        rest = &body[end + 3..];
    }
    out
}

fn assert_examples(doc: &str, label: &str) {
    let blocks = json_fences(doc);
    assert!(
        !blocks.is_empty(),
        "{label}: expected at least one ```json example"
    );
    let mut queries = 0usize;
    let mut commands = 0usize;
    for (i, block) in blocks.iter().enumerate() {
        // Keyword-only checks cannot catch args-wrapper drift; deserialize.
        if block.contains("\"request_id\"") {
            let cmd: WorkstreamCommand = serde_json::from_str(block)
                .unwrap_or_else(|e| panic!("{label} command example #{i} failed: {e}\n{block}"));
            assert_eq!(cmd.protocol_version, 1);
            assert!(!cmd.op.is_empty());
            assert!(!cmd.work_id.is_empty());
            assert!(cmd.args.is_object(), "command args must be an object");
            // Guard the exact drift this CR targets: query-shaped wrappers.
            assert!(
                block.contains("\"workstream_id\"")
                    && block.contains("\"coordinator_epoch\"")
                    && block.contains("\"expected_contract_hash\""),
                "{label} command #{i} missing top-level preconditions"
            );
            commands += 1;
        } else {
            let q: WorkstreamQuery = serde_json::from_str(block)
                .unwrap_or_else(|e| panic!("{label} query example #{i} failed: {e}\n{block}"));
            assert_eq!(q.protocol_version, 1);
            assert!(!q.op.is_empty());
            assert!(
                !block.contains("\"args\""),
                "{label} query #{i} must not wrap fields in args: {block}"
            );
            queries += 1;
        }
    }
    assert!(
        queries >= 2,
        "{label}: expected multiple query examples, got {queries}"
    );
    assert!(
        commands >= 2,
        "{label}: expected multiple command examples, got {commands}"
    );
}

#[test]
fn codex_cli_workflow_examples_match_production_types() {
    let doc = include_str!("../../../docs/dev/integrations/team-mcp-codex-cli.md");
    assert_examples(doc, "team-mcp-codex-cli");
}

#[test]
fn claude_code_workflow_examples_match_production_types() {
    let doc = include_str!("../../../docs/dev/integrations/team-mcp-claude-code.md");
    assert_examples(doc, "team-mcp-claude-code");
}
