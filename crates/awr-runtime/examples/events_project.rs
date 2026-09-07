use awr_core::*;
use awr_runtime::{EventQuery, Runtime};
use awr_store::Store;
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(Error::InvalidInput(
            "usage: events_project <root> <work key>".into(),
        ));
    }
    let root = std::path::Path::new(&args[0]);
    let mut store = Store::open_readonly(&root.join(".awr/state.db"))?;
    let project = store.project_by_root(root)?;
    let work = store.work_item(project.id, &args[1])?;
    let runtime = Runtime::attach(&mut store, project.id)?;
    let page = runtime.events(&EventQuery {
        work_item_id: Some(work.item.meta.id),
        limit: 10,
        ..Default::default()
    })?;
    println!(
        "work={}\nevents={}\nproject_revision={}",
        args[1],
        page.events.len(),
        page.project_revision
    );
    for event in page.events {
        println!(
            "{} r{} {} {}",
            event.id, event.project_revision, event.event_type, event.summary
        );
    }
    Ok(())
}
