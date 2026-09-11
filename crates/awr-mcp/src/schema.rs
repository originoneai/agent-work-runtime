use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{Value, json};

pub const TOOL_NAMES: [&str; 8] = [
    "awr_project_status",
    "awr_work_ready",
    "awr_work_get",
    "awr_context_compile",
    "awr_work_transition",
    "awr_event_append",
    "awr_evidence_record",
    "awr_search",
];

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn text() -> Value {
    json!({"type":"string"})
}
fn strings() -> Value {
    json!({"type":"array","items":{"type":"string"}})
}
fn optional(mut schema: Value) -> Value {
    let kind = schema["type"].clone();
    schema["type"] = json!([kind, "null"]);
    schema
}
fn branch() -> Value {
    json!({"type":["string","null"],"description":"Work-branch name/ID; main is explicit baseline. Omit for current selection."})
}
fn revision() -> Value {
    json!({"type":"integer","minimum":0,"maximum":i64::MAX})
}
fn levels() -> Value {
    json!({"type":"string","enum":["designed","implemented","locally_verified","real_environment_validated","release_candidate","released","unknown"]})
}
fn limit() -> Value {
    json!({"type":"integer","minimum":1,"maximum":100,"default":10})
}
fn tool(
    name: &'static str,
    description: &'static str,
    schema: Value,
    read_only: bool,
    destructive: bool,
) -> Tool {
    let mut tool = Tool::new(
        name,
        description,
        schema.as_object().expect("object schema").clone(),
    );
    tool.annotations = Some(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(destructive)
            .idempotent(read_only)
            .open_world(false),
    );
    tool
}
pub fn tools() -> Vec<Tool> {
    let completion = object(
        json!({
            "version":{"type":"integer","const":1},"source_sha":text(),"minimum_level":levels(),
            "acceptance":{"type":"array","items":object(json!({"criterion":text(),"evidence":strings()}), &["criterion","evidence"])},
            "required_evidence":strings(),
        }),
        &["version", "source_sha", "acceptance"],
    );
    vec![
        tool(
            TOOL_NAMES[0],
            "Read project progress, organization gaps, ordered repair actions and business readiness. Optional source_sha verifies completion reports. Never refresh persistent state; after source edits run awr source reindex before rechecking.",
            object(
                json!({"branch":branch(),"source_sha":optional(text()),"view":{"type":"string","enum":["full","summary"],"default":"full"},"work":strings(),"goal":optional(text()),"milestone":optional(text())}),
                &[],
            ),
            true,
            false,
        ),
        tool(
            TOOL_NAMES[1],
            "Read dependency-ready work and bounded exclusion diagnostics, including active runtime claims.",
            object(json!({"branch":branch(),"limit":limit()}), &[]),
            true,
            false,
        ),
        tool(
            TOOL_NAMES[2],
            "Read one work item, exact acceptance, dependencies and decision/evidence summaries.",
            object(
                json!({"work":text(),"branch":branch(),"source_sha":optional(text())}),
                &["work"],
            ),
            true,
            false,
        ),
        tool(
            TOOL_NAMES[3],
            "Compile the existing L1 context contract within budget. Incomplete context returns its gaps with isError. Explicit branch uses fork delta.",
            object(
                json!({
                    "work":optional(text()),"session":optional(text()),"detached":{"type":"boolean","default":false},"agent":optional(text()),"branch":branch(),
                    "goals":strings(),"paths":optional(strings()),"tags":optional(strings()),"source_sha":optional(text()),"intent":optional(text()),
                    "budget":{"type":["integer","null"],"minimum":1,"maximum":100000,"default":5000},"checkpoint":optional(text()),"after_revision":optional(revision()),
                }),
                &[],
            ),
            true,
            false,
        ),
        tool(
            TOOL_NAMES[4],
            "Apply one source work action through domain revision/fingerprint/claim gates. Complete requires every criterion's evidence mapping. Start sessions/claims via CLI.",
            object(
                json!({
                    "work":text(),"action":{"type":"string","enum":["progress","block","unblock","cancel","reopen","complete"]},"session":text(),"expected_revision":revision(),"reason":text(),
                    "next_action":optional(text()),"summary":optional(text()),"blocker":optional(text()),"completion":optional(completion),
                }),
                &["work", "action", "session", "expected_revision", "reason"],
            ),
            false,
            true,
        ),
        tool(
            TOOL_NAMES[5],
            "Append a bounded generic event. Reserved lifecycle events require their domain operation; payload is data, never executed.",
            object(
                json!({
                    "expected_revision":revision(),"work":optional(text()),"session":optional(text()),"branch":branch(),
                    "event_type":{"type":"string","minLength":1,"maxLength":awr_core::EVENT_TYPE_CAP,"pattern":"^[A-Za-z0-9_.:-]+$"},
                    "importance":{"type":["string","null"],"enum":["low","normal","high","critical",null]},
                    "summary":{"type":"string","minLength":1,"maxLength":awr_core::EVENT_SUMMARY_CAP,"description":"Nonempty summary, at most 8192 UTF-8 bytes."},
                    "payload":optional(awr_core::generic_event_payload_schema()),
                }),
                &["expected_revision", "event_type", "summary"],
            ),
            false,
            false,
        ),
        tool(
            TOOL_NAMES[6],
            "Record evidence bindings and report reference. Verification fields are caller assertions; no report command is executed.",
            object(
                json!({
                    "expected_revision":revision(),"external_key":text(),"work":optional(text()),"evidence_type":text(),"level":levels(),"summary":text(),"locator":text(),
                    "sha256":optional(text()),"source_sha":optional(text()),"command":optional(text()),"scope":{"type":"array","items":{"type":"string"},"minItems":1},"branch":branch(),"verified_at":{"type":["integer","null"]},
                }),
                &[
                    "expected_revision",
                    "external_key",
                    "evidence_type",
                    "level",
                    "summary",
                    "locator",
                    "scope",
                ],
            ),
            false,
            false,
        ),
        tool(
            TOOL_NAMES[7],
            "Search bounded summaries with structured filters. Search cache is built only in RAM; event payloads and file bodies are excluded.",
            object(
                json!({
                    "text":optional(text()),"kind":{"type":["string","null"],"enum":["goal","plan","rule","work_item","work","decision","evidence","event",null]},
                    "status":optional(text()),"work":optional(text()),"limit":limit(),
                }),
                &[],
            ),
            true,
            false,
        ),
    ]
}

pub(crate) fn shared_tools() -> Vec<Tool> {
    let mut catalog = tools();
    for tool in &mut catalog {
        let schema = std::sync::Arc::make_mut(&mut tool.input_schema);
        schema["properties"].as_object_mut().unwrap().insert("project".into(),
            json!({"type":"string","description":"Registered project key from awr_projects_list. Required on every call; never a filesystem path."}));
        schema["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("project"));
    }
    catalog.insert(0, tool("awr_projects_list", "List this authenticated client's registered project keys and access. Does not read project source files.", object(json!({}), &[]), true, false));
    catalog
}
