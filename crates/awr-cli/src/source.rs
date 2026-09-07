use awr_core::{AuthorityMode, Error, Freshness, Result};
use awr_source::{IndexReport, Manifest, ProjectConfig, SourceSpec, index_project, scan_project};
use awr_store::Store;
use clap::Subcommand;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Subcommand)]
pub enum SourceCommand {
    /// Read registered sources and their last observed freshness without changing the database.
    List,
    /// Refresh source availability and report pending changes without parsing new facts.
    Scan,
    /// Reconcile source content and parsing configuration with the projection database.
    Reindex {
        #[arg(long)]
        force: bool,
    },
}

fn runtime_dir(root: &Path, create: bool) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let path = root.join(".awr");
    if create && !path.exists() {
        fs::create_dir(&path)?;
    }
    let actual = path
        .canonicalize()
        .map_err(|e| Error::SourceUnavailable(format!("{}: {e}", path.display())))?;
    if !actual.starts_with(&root) || !actual.is_dir() {
        return Err(Error::RuleViolation(
            "runtime directory escapes project root".into(),
        ));
    }
    Ok(actual)
}

pub fn initialize(
    root: &Path,
    manifest_path: Option<&Path>,
    accept: bool,
    json_output: bool,
) -> Result<()> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(Error::InvalidInput(
            "project root must be a directory".into(),
        ));
    }
    let existing_path = root.join(".awr/project.toml");
    let existing = if existing_path.exists() {
        Some(Manifest::load(&root)?)
    } else {
        None
    };
    let (candidate, candidates, ambiguous) = if let Some(path) = manifest_path {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        let bytes = awr_source::read_capped(&path, 64 * 1024)?;
        let candidate = Manifest::parse(
            std::str::from_utf8(&bytes).map_err(|e| Error::InvalidInput(e.to_string()))?,
        )?;
        (Some(candidate), vec![], vec![])
    } else if let Some(existing) = &existing {
        (Some(existing.clone()), vec![], vec![])
    } else {
        discover(&root)?
    };
    if let (Some(existing), Some(candidate)) = (&existing, &candidate) {
        if serde_json::to_value(existing)? != serde_json::to_value(candidate)? {
            return Err(Error::SourceConflict("project.toml already exists with another mapping; initialization never overwrites it".into()));
        }
    }
    if !accept {
        let toml = candidate
            .as_ref()
            .map(toml::to_string_pretty)
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let preview = json!({"status":"preview","configuration_exists":existing.is_some(),"requires_accept":true,
            "candidates":candidates,"ambiguous_domains":ambiguous,"authority_mapping":candidate,"manifest_toml":toml});
        if json_output {
            println!("{}", serde_json::to_string_pretty(&preview)?);
        } else {
            println!("Source authority mapping preview (no files changed).");
            if let Some(toml) = toml {
                println!("\n{toml}\nUse init --accept with this mapping to initialize.");
            } else {
                println!("{}", serde_json::to_string_pretty(&preview)?);
                println!("Supply an explicit --manifest, then use --accept.");
            }
        }
        return Ok(());
    }
    let manifest = candidate.ok_or_else(|| {
        Error::InvalidInput(if ambiguous.is_empty() {
            "no source candidates; supply an explicit --manifest".into()
        } else {
            format!(
                "multiple authority candidates for {}; supply an explicit --manifest",
                ambiguous.join(", ")
            )
        })
    })?;
    manifest.validate()?;
    if existing.is_none() && root.join(".awr/state.db").exists() {
        return Err(Error::SourceConflict("database exists without project.toml; restore its matching manifest before initializing".into()));
    }
    // Check the ignore target before creating configuration; append only missing runtime entries.
    let ignore = root.join(".gitignore");
    if fs::symlink_metadata(&ignore).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(Error::RuleViolation(
            "initialization cannot append to a symlinked .gitignore".into(),
        ));
    }
    let previous_ignore = if ignore.exists() {
        fs::read_to_string(&ignore)?
    } else {
        String::new()
    };
    let needed = [
        ".awr/state.db",
        ".awr/state.db-*",
        ".awr/artifacts/",
        ".awr/cache/",
    ];
    let missing: Vec<_> = needed
        .iter()
        .filter(|entry| !previous_ignore.lines().any(|line| line.trim() == **entry))
        .collect();
    let runtime = runtime_dir(&root, true)?;
    if existing.is_none() {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(runtime.join("project.toml"))?;
        file.write_all(
            toml::to_string_pretty(&manifest)
                .map_err(|e| Error::InvalidInput(e.to_string()))?
                .as_bytes(),
        )?;
        file.sync_all()?;
    }
    if !missing.is_empty() {
        let mut file = OpenOptions::new().append(true).create(true).open(ignore)?;
        if !previous_ignore.is_empty() && !previous_ignore.ends_with('\n') {
            writeln!(file)?;
        }
        writeln!(file, "\n# AWR local runtime state")?;
        for entry in missing {
            writeln!(file, "{entry}")?;
        }
        file.sync_all()?;
    }
    let mut store = Store::open(&runtime.join("state.db"))?;
    let report = index_project(&mut store, &root, &manifest, false)?;
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"initialized":true,"configuration_created":existing.is_none(),
        "manifest":runtime.join("project.toml"),"authority_mapping":manifest.sources,"index":report})
            )?
        );
    } else {
        println!(
            "Initialized {} using {}",
            manifest.project.name,
            runtime.join("project.toml").display()
        );
        print_report(&report);
    }
    if !report.ok {
        return Err(Error::SourceStale(
            "project initialized with source issues; resolve them and run source reindex".into(),
        ));
    }
    Ok(())
}

fn discover(root: &Path) -> Result<(Option<Manifest>, Vec<Value>, Vec<String>)> {
    let locations: &[(&str, &str, &[&str])] = &[
        (
            "goal",
            "markdown-heading-v1",
            &["GOALS.md", "docs/GOALS.md"],
        ),
        ("plan", "markdown-heading-v1", &["PLAN.md", "docs/PLAN.md"]),
        ("rules", "markdown-rules-v1", &["RULES.md", "docs/RULES.md"]),
        (
            "ledger",
            "yaml-ledger-v1",
            &[
                "ledger/work-ledger.yaml",
                "work-ledger.yaml",
                "ledger.yaml",
                "ledger/work-ledger.yml",
                "work-ledger.yml",
            ],
        ),
        (
            "decisions",
            "markdown-directory-v1",
            &["docs/decisions", "docs/adr", "docs/rfc", "adr"],
        ),
    ];
    let mut sources = vec![];
    let mut candidates = vec![];
    let mut ambiguous = vec![];
    for (domain, adapter, paths) in locations {
        let found: Vec<_> = paths
            .iter()
            .filter(|path| {
                if *domain == "decisions" {
                    root.join(path).is_dir()
                } else {
                    root.join(path).is_file()
                }
            })
            .collect();
        if found.len() > 1 && *domain != "decisions" {
            ambiguous.push((*domain).to_owned());
        }
        for path in found {
            candidates.push(json!({"domain":domain,"path":path,"adapter":adapter}));
            sources.push(SourceSpec {
                domain: (*domain).into(),
                role: if *domain == "decisions" {
                    "supporting"
                } else {
                    "primary"
                }
                .into(),
                path: Some(PathBuf::from(path)),
                locator: None,
                adapter: (*adapter).into(),
                options: Default::default(),
            });
        }
    }
    let name = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("AWR project")
        .to_owned();
    let manifest = if sources.is_empty() || !ambiguous.is_empty() {
        None
    } else {
        Some(Manifest {
            project: ProjectConfig {
                name,
                external_key: None,
                authority_mode: AuthorityMode::SourceFirst,
                authorized_roots: vec![],
            },
            sources,
        })
    };
    Ok((manifest, candidates, ambiguous))
}

pub fn run(root: &Path, command: &SourceCommand, json_output: bool) -> Result<()> {
    let root = root.canonicalize()?;
    let manifest = Manifest::load(&root)?;
    let runtime = runtime_dir(&root, false)?;
    match command {
        SourceCommand::List => {
            let store = Store::open_readonly(&runtime.join("state.db"))?;
            let project = store.project_by_root(&root)?;
            let sources = store.sources(project.id)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &json!({"project_id":project.id,"project_revision":project.project_revision,
                "freshness_basis":"last_scan_or_index","mappings":manifest.sources,"sources":sources})
                    )?
                );
            } else {
                println!("Sources at last observation; use source scan to refresh:");
                for source in sources {
                    println!(
                        "{} {} {} r{} {}",
                        source.domain,
                        source.role,
                        freshness(source.freshness),
                        source.revision,
                        source.locator
                    );
                }
            }
        }
        SourceCommand::Scan | SourceCommand::Reindex { .. } => {
            let mut store = Store::open(&runtime.join("state.db"))?;
            let report = match command {
                SourceCommand::Scan => scan_project(&mut store, &root, &manifest)?,
                SourceCommand::Reindex { force } => {
                    index_project(&mut store, &root, &manifest, *force)?
                }
                _ => unreachable!(),
            };
            if json_output {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_report(&report);
            }
            if !report.ok {
                return Err(Error::SourceStale(
                    "source operation incomplete; inspect reported issues".into(),
                ));
            }
        }
    }
    Ok(())
}
fn freshness(value: Freshness) -> &'static str {
    match value {
        Freshness::Fresh => "fresh",
        Freshness::Stale => "stale",
        Freshness::Unavailable => "unavailable",
    }
}
fn print_report(report: &IndexReport) {
    println!(
        "Indexed: {}; unchanged: {}; pending: {}; retired: {}; project revision: {}",
        report.indexed, report.unchanged, report.pending, report.retired, report.project_revision
    );
    let mut warnings = BTreeMap::new();
    for source in &report.sources {
        println!(
            "{} {} {}",
            source.action,
            freshness(source.freshness),
            source.locator
        );
        for warning in &source.warnings {
            warnings.insert(format!("{}: {warning}", source.locator), ());
        }
    }
    for warning in warnings.keys() {
        println!("warning: {warning}");
    }
    for issue in &report.issues {
        println!("{}: {}: {}", issue.code, issue.mapping, issue.message);
    }
}
