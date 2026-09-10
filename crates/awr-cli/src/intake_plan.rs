//! Read-only effect inventories used to bind host intake consent to source bytes.
use awr_core::{Error, Result};
use awr_source::{Manifest, fingerprint, source_adapter, source_read_cap};
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

pub const IGNORE_ENTRIES: &[&str] = &[
    ".awr/state.db",
    ".awr/state.db-*",
    ".awr/artifacts/",
    ".awr/mutations/",
    ".awr/cache/",
    ".awr/clients/",
    ".awr/executions/",
];

#[derive(Debug, Clone, Serialize)]
pub struct FileEffect {
    pub path: String,
    pub action: &'static str,
    pub before_fingerprint: Option<String>,
    pub after_fingerprint: String,
    pub after_text: String,
}

/// Read a bounded regular file without accepting a substituted symlink.
pub fn current(root: &Path, relative: &str, cap: u64) -> Result<Option<Vec<u8>>> {
    let path = root.join(relative);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => Ok(Some(awr_source::read_source_capped(&path, cap)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn effect(root: &Path, path: &str, after_text: String) -> Result<FileEffect> {
    let before = current(root, path, awr_source::YAML_READ_CAP)?;
    let action = match &before {
        None => "create",
        Some(bytes) if *bytes == after_text.as_bytes() => "no_change",
        Some(_) => "replace",
    };
    Ok(FileEffect {
        path: path.into(),
        action,
        before_fingerprint: before.map(|b| fingerprint(&b)),
        after_fingerprint: fingerprint(after_text.as_bytes()),
        after_text,
    })
}

pub fn ignore_effect(root: &Path) -> Result<FileEffect> {
    let previous = current(root, ".gitignore", awr_source::YAML_READ_CAP)?;
    let mut text = String::from_utf8(previous.clone().unwrap_or_default())
        .map_err(|_| Error::InvalidInput(".gitignore must be UTF-8".into()))?;
    let missing: Vec<_> = IGNORE_ENTRIES
        .iter()
        .filter(|entry| !text.lines().any(|line| line.trim() == **entry))
        .collect();
    if !missing.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("\n# AWR local runtime state\n");
        for entry in missing {
            text.push_str(entry);
            text.push('\n');
        }
    }
    effect(root, ".gitignore", text)
}

pub fn verify_file(root: &Path, effect: &FileEffect) -> Result<()> {
    let actual =
        current(root, &effect.path, awr_source::YAML_READ_CAP)?.map(|bytes| fingerprint(&bytes));
    if actual != effect.before_fingerprint {
        return Err(Error::SourceConflict(format!(
            "{} changed after preview",
            effect.path
        )));
    }
    Ok(())
}

pub fn preview(
    root: &Path,
    manifest: &Manifest,
    generated: &BTreeMap<String, String>,
    configure: bool,
) -> Result<Value> {
    let mut writes = Vec::new();
    let rendered =
        toml::to_string_pretty(manifest).map_err(|e| Error::InvalidInput(e.to_string()))?;
    let existing = current(root, ".awr/project.toml", 64 * 1024)?;
    // Repeated initialization keeps the exact existing manifest, including comments.
    let equivalent = existing.as_ref().is_some_and(|bytes| {
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|text| Manifest::parse(text).ok())
            .is_some_and(|current| {
                serde_json::to_value(&current).ok() == serde_json::to_value(manifest).ok()
            })
    });
    let after = if (!configure || equivalent)
        && let Some(ref bytes) = existing
    {
        String::from_utf8(bytes.clone())
            .map_err(|_| Error::InvalidInput("manifest must be UTF-8".into()))?
    } else {
        rendered
    };
    writes.push(effect(root, ".awr/project.toml", after)?);
    if !configure {
        writes.push(ignore_effect(root)?);
    }
    for (path, body) in generated {
        writes.push(effect(root, path, body.clone())?);
    }
    let mut snapshots = Vec::new();
    let mut issues = Vec::new();
    for spec in &manifest.sources {
        if let Some(body) = spec
            .path
            .as_ref()
            .and_then(|p| generated.get(&p.to_string_lossy().into_owned()))
        {
            snapshots.push(json!({"domain":spec.domain,"path":spec.path,"proposed":true,"fingerprint":fingerprint(body.as_bytes())}));
            continue;
        }
        let adapter = source_adapter(&spec.adapter)?;
        match adapter.discover(root, manifest, spec) {
            Ok(locators) => {
                for locator in locators {
                    match locator.read(root, source_read_cap(&spec.adapter)?) {
                        Ok(snapshot) => snapshots.push(json!({"domain":spec.domain,"locator":snapshot.locator,"fingerprint":snapshot.fingerprint})),
                        Err(error) => issues.push(json!({"domain":spec.domain,"source":spec,"error":error.report()})),
                    }
                }
            }
            Err(error) => {
                issues.push(json!({"domain":spec.domain,"source":spec,"error":error.report()}))
            }
        }
    }
    snapshots.sort_by_key(|s| s.to_string());
    let mut runtime_effects = vec![
        json!({"path":".awr/state.db","action":"create_or_refresh_and_migrate_if_compatible"}),
        json!({"path":".awr/state.db-wal","action":"sqlite_wal_if_needed"}),
        json!({"path":".awr/state.db-shm","action":"sqlite_shared_memory_if_needed"}),
    ];
    if configure {
        runtime_effects.extend([
            json!({"path_pattern":".awr/mutations/source-config-*/","action":"before_after_snapshots_and_durable_receipt"}),
            json!({"path_pattern":".awr/mutations/*.lock","action":"configuration_writer_lock"}),
            json!({"path_pattern":".awr/config-*.tmp","action":"temporary_atomic_replacement"}),
        ]);
    } else if !generated.is_empty() {
        runtime_effects
            .push(json!({"path_pattern":".awr/intake-*/","action":"temporary_intake_staging"}));
    }
    let mut plan = json!({
        "version":1, "operation":if configure {"source.configure"} else {"init"},
        "project_root":root, "authority_mapping":manifest, "writes":writes,
        "source_snapshots":snapshots, "source_issues":issues,
        "runtime_effects":runtime_effects,
        "source_write_performed":false, "runtime_write_performed":false,
        "atomicity":"individual_files_only; partial_initialization_can_remain_on_failure"
    });
    let digest = fingerprint(&serde_json::to_vec(&plan)?);
    plan["fingerprint"] = json!(digest);
    Ok(plan)
}

pub fn check_expected(plan: &Value, expected: Option<&str>) -> Result<()> {
    if let Some(expected) = expected {
        if plan["fingerprint"].as_str() != Some(expected) {
            return Err(Error::SourceConflict("intake preview changed; refresh and review source/configuration/ignore effects before accepting".into()));
        }
    }
    Ok(())
}
