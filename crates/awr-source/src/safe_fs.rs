//! Open already-authorized, canonical source paths without following replacement links.
//! Authorization happens in Locator/Manifest; these operations bind filesystem access
//! to directory handles so a later path lookup cannot redirect it through a symlink.
use awr_core::{Error, Result};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use std::{
    fs::File,
    path::{Component, Path, PathBuf},
};

fn unavailable(path: &Path, error: std::io::Error) -> Error {
    Error::SourceUnavailable(format!(
        "{}: exact source path could not be opened: {error}",
        path.display()
    ))
}

/// The input must already be an absolute authorized path. Each component is opened
/// separately with no-follow semantics; a multi-component no-follow call only protects
/// its final component. This also handles drive/UNC roots on Windows.
pub fn open_dir_exact(path: &Path) -> Result<Dir> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error::RuleViolation(format!(
            "{}: exact directory needs an absolute path without parent traversal",
            path.display()
        )));
    }
    let mut parts = path.components().peekable();
    let mut anchor = PathBuf::new();
    while let Some(Component::Prefix(_) | Component::RootDir) = parts.peek() {
        anchor.push(parts.next().unwrap().as_os_str());
    }
    let mut dir =
        Dir::open_ambient_dir(anchor, ambient_authority()).map_err(|e| unavailable(path, e))?;
    for component in parts {
        match component {
            Component::Normal(name) => {
                dir = dir
                    .open_dir_nofollow(name)
                    .map_err(|e| unavailable(path, e))?;
            }
            Component::CurDir => (),
            _ => {
                return Err(Error::RuleViolation(format!(
                    "{}: invalid exact path component",
                    path.display()
                )));
            }
        }
    }
    Ok(dir)
}

/// Open the final file relative to a held parent and refuse a substituted leaf link.
pub fn open_file_exact(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidInput("source file has no parent".into()))?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::InvalidInput("source file has no name".into()))?;
    let dir = open_dir_exact(parent)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = dir
        .open_with(name, &options)
        .map_err(|e| unavailable(path, e))?;
    Ok(file.into_std())
}
