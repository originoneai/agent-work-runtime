use awr_source::{Locator, Manifest};
fn main() -> awr_core::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 {
        return Err(awr_core::Error::InvalidInput(
            "usage: read_source <project-root> <manifest-path> <source-index>".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]);
    let text = std::fs::read_to_string(&args[1])?;
    let manifest = Manifest::parse(&text)?;
    let index = args[2]
        .to_str()
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or_else(|| awr_core::Error::InvalidInput("source index must be an integer".into()))?;
    let spec = manifest
        .sources
        .get(index)
        .ok_or_else(|| awr_core::Error::NotFound("source index".into()))?;
    let locator = Locator::from_spec(root, &manifest, spec)?;
    let cap = if spec.adapter == "yaml-ledger-v1" {
        4 * 1024 * 1024
    } else {
        2 * 1024 * 1024
    };
    let snapshot = locator.read(root, cap)?;
    println!(
        "{}",
        serde_json::json!({"project":manifest.project.name,"source_count":manifest.sources.len(),
        "domain":spec.domain,"role":spec.role,"adapter":spec.adapter,"locator":snapshot.locator,
        "fingerprint":snapshot.fingerprint,"bytes":snapshot.bytes.len(),"lines":snapshot.text()?.lines().count()})
    );
    Ok(())
}
