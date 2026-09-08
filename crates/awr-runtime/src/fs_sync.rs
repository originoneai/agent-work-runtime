use awr_core::Result;
use cap_std::fs::Dir;

/// Flush the held directory without resolving its original pathname again.
/// Linux capability directories can use O_PATH, whose descriptor cannot be
/// fsync'd. Opening "." relative to that handle supplies a readable descriptor
/// for the same directory, even if an ancestor or its old name was replaced.
#[cfg(unix)]
pub(crate) fn sync_directory(directory: &Dir) -> Result<()> {
    directory.open(".")?.sync_all()?;
    Ok(())
}

// Keep the existing non-Unix boundary: file contents are synced, but this does
// not claim a portable directory-fsync or power-loss guarantee on Windows.
#[cfg(not(unix))]
pub(crate) fn sync_directory(_directory: &Dir) -> Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use awr_core::Id;
    use awr_source::open_dir_exact;
    use std::{fs, os::unix::fs::symlink, path::PathBuf};

    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn held_directory_remains_syncable_after_its_path_is_replaced() {
        let fixture =
            Temporary(std::env::temp_dir().join(format!("awr-directory-sync-{}", Id::new())));
        let original = fixture.0.join("original");
        fs::create_dir_all(&original).unwrap();
        let held = open_dir_exact(&original.canonicalize().unwrap()).unwrap();
        fs::write(original.join("pending.txt"), "retained directory entry").unwrap();
        sync_directory(&held).unwrap();
        fs::rename(&original, fixture.0.join("retained")).unwrap();
        symlink(fixture.0.join("absent"), &original).unwrap();
        sync_directory(&held).unwrap();
        assert_eq!(
            held.read_to_string("pending.txt").unwrap(),
            "retained directory entry"
        );
        assert!(!fixture.0.join("absent").exists());
    }
}
