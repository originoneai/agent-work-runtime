"""Deterministic synthetic issue-tracker project; no development history or padding."""
from pathlib import Path
import yaml

FEATURES = [
    ("request intake", "Keep the original request and its attachments together", "A new request retains its text and attachment references"),
    ("owner routing", "Assign a responsible team and expose unassigned requests", "Every assigned request has exactly one responsible team"),
    ("customer search", "Find requests by customer name and current status", "Search returns matching requests without unrelated customer records"),
    ("priority queue", "Order outstanding work by urgency and arrival time", "Equal-priority requests retain their arrival order"),
    ("due date reminders", "Notify the owner before an unresolved request becomes overdue", "Resolved requests do not receive overdue reminders"),
    ("review workflow", "Let a separate reviewer accept a proposed resolution", "A rejected resolution returns to its author with a reason"),
    ("attachment preview", "Display supported files without changing the stored original", "Unsupported attachments keep a working download link"),
    ("activity history", "Show who changed a request and why", "Every recorded status change includes its actor and reason"),
    ("team reporting", "Summarize resolved and outstanding requests by team", "Totals reconcile with the requests visible to that team"),
    ("data export", "Export selected request fields for downstream analysis", "Exported rows preserve source identifiers and timestamps"),
]
STAGES = [
    ("data model", "Document the stored fields and their nullability"),
    ("input validation", "Implement validation and user-readable field errors"),
    ("service API", "Connect the request handler to the persisted model"),
    ("list view", "Show the result in the team's request list"),
    ("detail view", "Explain the current state in the request detail page"),
    ("edit flow", "Persist edits and preserve unrelated fields"),
    ("empty state", "Explain the next useful action when no records match"),
    ("failure recovery", "Retain user input after a recoverable service failure"),
    ("keyboard access", "Make the main interaction usable from the keyboard"),
    ("migration", "Migrate existing records with explicit defaults"),
    ("integration check", "Exercise the flow with a related team feature"),
    ("documentation", "Describe the workflow with a complete user example"),
    ("review feedback", "Resolve the review findings and record the rationale"),
    ("acceptance check", "Check the result against the stated acceptance criteria"),
    ("delivery", "Prepare the final example and handoff instructions"),
]
RULES = [
    {"key": "source", "text": "Preserve the original customer request when deriving summaries or exports."},
    {"key": "review", "text": "A resolution needs an independent reviewer before it is marked accepted."},
]


def create(root):
    root = Path(root)
    root.mkdir(parents=True, exist_ok=False)
    work = []
    for index in range(150):
        feature, purpose, criterion = FEATURES[index % len(FEATURES)]
        stage, action = STAGES[index // len(FEATURES)]
        key = f"TASK-{index+1:03d}"
        status = ("blocked", "planned", "in_progress")[index % 3] if index < 39 else "completed"
        dependencies = [f"TASK-{index:03d}"] if index < 39 and index % 3 == 1 else []
        row = {
            "id": key, "title": f"{feature.title()}: {stage}", "status": status,
            "goal": f"goal#g{index % 6 + 1}", "milestone": f"M{index % 6 + 1}",
            "priority": f"P{index % 3}", "required": True,
            "summary": f"{purpose}. This task covers the {stage} portion of the workflow.",
            "acceptance": [criterion + ".", action + "."],
            "depends_on": dependencies, "next_action": f"{action} for {feature}.",
            "evidence": [],
        }
        if status == "blocked":
            row["blocker"] = f"The team must confirm the {feature} field definitions before implementation."
        work.append(row)
    document = {"milestones": [{"id": f"M{i}", "title": f"Delivery group {i}", "status": "in_progress",
                                "acceptance": ["Complete the linked tasks and retain their review results."]}
                               for i in range(1,7)], "work_items": work}
    (root/"work-ledger.yaml").write_text(yaml.safe_dump(document,sort_keys=False,allow_unicode=True), encoding="utf-8")
    (root/"GOALS.md").write_text("\n\n".join(
        f"# Deliver support workflow group {i} {{#g{i} status=active}}\n\nHelp the team handle requests from intake through reviewed resolution."
        for i in range(1,7)) + "\n", encoding="utf-8")
    (root/"RULES.md").write_text("\n\n".join(
        f"# {r['key'].title()} {{#{r['key']} severity=hard scope=project value=*}}\n\n{r['text']}"
        for r in RULES) + "\n", encoding="utf-8")
    (root/"project.toml").write_text('''[project]
name = "Synthetic support tracker"
external_key = "public-context-benchmark"
authority_mode = "source_first"

[[sources]]
domain = "goal"
role = "primary"
path = "GOALS.md"
adapter = "markdown-heading-v1"
[sources.options]
key_prefix = "goal"

[[sources]]
domain = "rules"
role = "primary"
path = "RULES.md"
adapter = "markdown-rules-v1"

[[sources]]
domain = "ledger"
role = "primary"
path = "work-ledger.yaml"
adapter = "yaml-ledger-v1"
''', encoding="utf-8")
    return work
