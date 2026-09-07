use crate::{Freshness, Revision, Rule, ScopeKind, Source};
use globset::GlobBuilder;
use serde::{Deserialize, Serialize};

/// A projection and its source state from the same database snapshot.
/// Freshness is based on the latest source scan/index, not a live filesystem check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Projected<T> {
    pub item: T,
    pub source: Source,
    pub project_revision: Revision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub work_item_key: String,
    pub required_only: bool,
    pub dependencies: Vec<Projected<crate::WorkItem>>,
    pub edges: Vec<Projected<crate::Edge>>,
    pub missing_keys: Vec<String>,
    pub cycle_keys: Vec<String>,
    pub project_revision: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ReadinessDiagnostic {
    pub code: String,
    pub work_item_key: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkReadiness {
    pub work: Projected<crate::WorkItem>,
    pub ready: bool,
    pub dependencies: DependencyGraph,
    pub active_claims: Vec<crate::Claim>,
    pub diagnostics: Vec<ReadinessDiagnostic>,
    pub branch_id: Option<crate::Id>,
    pub evaluated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyReport {
    pub ready: Vec<WorkReadiness>,
    /// All nonterminal items that cannot currently be selected, with explicit reasons.
    pub blocked: Vec<WorkReadiness>,
    pub project_revision: Revision,
    pub branch_id: Option<crate::Id>,
    pub evaluated_at: i64,
}

/// None means context was not supplied; Some([]) means a known empty set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuleContext {
    pub project_key: Option<String>,
    pub paths: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub work_item_key: Option<String>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Applicability {
    Applicable,
    NotApplicable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatch {
    pub rule: Projected<Rule>,
    pub applicability: Applicability,
    pub reasons: Vec<String>,
}

/// Match project-relative paths, with case-sensitive globs and literal separators.
/// A trailing slash selects a directory's descendants. Invalid/unsafe paths are unknown.
pub fn match_path_scope(pattern: &str, paths: &[String]) -> std::result::Result<bool, String> {
    fn relative(path: &str) -> bool {
        !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && !path.contains(':')
            && !path.split('/').any(|part| part == "..")
    }
    if !relative(pattern) || paths.iter().any(|path| !relative(path)) {
        return Err("path scope and context must use project-relative forward-slash paths".into());
    }
    let clean = |path: &str| {
        path.split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect::<Vec<_>>()
            .join("/")
    };
    let mut pattern_clean = clean(pattern);
    if pattern.ends_with('/') {
        pattern_clean.push_str("/**");
    }
    let matcher = GlobBuilder::new(&pattern_clean)
        .literal_separator(true)
        .backslash_escape(true)
        .build()
        .map_err(|e| format!("invalid path scope: {e}"))?
        .compile_matcher();
    Ok(paths.iter().any(|path| matcher.is_match(clean(path))))
}

impl Projected<Rule> {
    /// Keep every rule in the result, including unresolved metadata and missing context.
    pub fn evaluate(self, context: &RuleContext) -> RuleMatch {
        let mut reasons = self.item.unresolved.clone();
        let scope_match = match &self.item.scope {
            None => Err("rule scope is unknown".into()),
            Some(scope) if scope.value.trim().is_empty() => Err("rule scope value is empty".into()),
            Some(scope) => {
                let equals = |value: &Option<String>, label: &str| {
                    value
                        .as_ref()
                        .map(|value| value == &scope.value)
                        .ok_or_else(|| format!("{label} context is unknown"))
                };
                match scope.kind {
                    ScopeKind::Project if scope.value == "*" => Ok(true),
                    ScopeKind::Project => equals(&context.project_key, "project"),
                    ScopeKind::WorkItem => equals(&context.work_item_key, "work item"),
                    ScopeKind::Agent => equals(&context.agent_id, "agent"),
                    ScopeKind::Tag => context
                        .tags
                        .as_ref()
                        .map(|tags| tags.contains(&scope.value))
                        .ok_or_else(|| "tag context is unknown".into()),
                    ScopeKind::Path => context
                        .paths
                        .as_ref()
                        .ok_or_else(|| "path context is unknown".into())
                        .and_then(|paths| match_path_scope(&scope.value, paths)),
                }
            }
        };
        if self.item.severity.is_none() {
            reasons.push("rule severity is unknown".into());
        }
        if self.source.freshness != Freshness::Fresh {
            reasons.push(format!("rule source is {:?}", self.source.freshness));
        }
        let applicability = match scope_match {
            Ok(false) if self.source.freshness == Freshness::Fresh => Applicability::NotApplicable,
            Ok(true) if reasons.is_empty() => Applicability::Applicable,
            Err(reason) => {
                reasons.push(reason);
                Applicability::Unknown
            }
            _ => Applicability::Unknown,
        };
        reasons.sort();
        reasons.dedup();
        RuleMatch {
            rule: self,
            applicability,
            reasons,
        }
    }
}
