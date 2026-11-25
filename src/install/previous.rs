//! The copy `install` keeps of what it replaces, so a new version that does not keep
//! answering can be swapped back for the one before it.
//!
//! What breaks if you change it: a file `install` writes as root that is not listed in
//! [`KEPT`] stays the new version's after a put-back, mixing two versions. Adding a path
//! `install` writes means adding it here.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use super::{
    BINARY_PATH, GATEKEEPER_PLIST_PATH, InstallError, LEGACY_UPDATE_SCRIPT_PATH, NEWSYSLOG_PATH,
    UPDATE_PLIST_PATH, UPGRADE_PLIST_PATH, clear_unless, io_error, make_dir, remove_if_present,
    set_owner_and_mode, staged_path,
};
use crate::upgrade::state::PREVIOUS_DIR;

/// Everything `install` replaces that decides what runs. Each name is distinct, so the copies
/// sit side by side in [`PREVIOUS_DIR`].
const KEPT: [&str; 6] = [
    BINARY_PATH,
    GATEKEEPER_PLIST_PATH,
    UPDATE_PLIST_PATH,
    UPGRADE_PLIST_PATH,
    LEGACY_UPDATE_SCRIPT_PATH,
    NEWSYSLOG_PATH,
];

/// Which of [`KEPT`] existed before the install, and their modes.
pub(super) struct Previous {
    existed: Vec<(&'static str, u32)>,
}

impl Previous {
    /// Copies what is installed now into [`PREVIOUS_DIR`], replacing the copy kept by the
    /// install before. `None` when nothing is installed yet, so there is nothing to go back to.
    pub(super) fn keep() -> Result<Option<Self>, InstallError> {
        if !Path::new(BINARY_PATH).is_file() {
            return Ok(None);
        }
        let dir = Path::new(PREVIOUS_DIR);
        clear_unless(dir, |_| false)?;
        make_dir(dir, 0o755, 0, 0)?;
        let mut existed = Vec::new();
        for path in KEPT {
            let Ok(meta) = fs::symlink_metadata(path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            let mode = meta.permissions().mode() & 0o777;
            let copy = copy_path(path);
            fs::copy(path, &copy).map_err(|source| io_error(&copy, source))?;
            set_owner_and_mode(&copy, mode, 0, 0)?;
            existed.push((path, mode));
        }
        Ok(Some(Self { existed }))
    }

    /// Puts every kept file back where it was and removes what the new version added.
    pub(super) fn restore(&self) -> Result<(), InstallError> {
        for path in KEPT {
            let target = Path::new(path);
            match self.existed.iter().find(|(kept, _)| *kept == path) {
                Some(&(_, mode)) => {
                    // Beside the target and renamed over it, as install writes: the program
                    // being replaced may still be running.
                    let staged = staged_path(target);
                    remove_if_present(&staged)?;
                    fs::copy(copy_path(path), &staged)
                        .map_err(|source| io_error(&staged, source))?;
                    set_owner_and_mode(&staged, mode, 0, 0)?;
                    fs::rename(&staged, target).map_err(|source| io_error(target, source))?;
                }
                None => remove_if_present(target)?,
            }
        }
        Ok(())
    }

    /// Whether `path` existed before the install.
    pub(super) fn kept(&self, path: &str) -> bool {
        self.existed.iter().any(|(kept, _)| *kept == path)
    }
}

fn copy_path(path: &str) -> PathBuf {
    let name = Path::new(path).file_name().unwrap_or_default();
    Path::new(PREVIOUS_DIR).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two copies with one name would overwrite each other.
    #[test]
    fn every_kept_file_has_its_own_name() {
        let mut names: Vec<PathBuf> = KEPT.iter().map(|path| copy_path(path)).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), KEPT.len());
    }
}
