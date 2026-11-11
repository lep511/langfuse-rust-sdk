//! Safety checks of installation directories: inside the project, not
//! overlapping each other or a locked skill, and away from the lockfile.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{CliError, Result};
use crate::skills::lock::SkillsLock;
use crate::skills::paths::{contains, physical, resolve};

/// Check `targets` (absolute directories) before anything is written.
pub fn validate(
    cwd: &Path,
    targets: &[PathBuf],
    lock: &SkillsLock,
    lockfile: Option<&Path>,
) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }
    let selected: HashSet<&PathBuf> = targets.iter().collect();
    if selected.len() != targets.len() {
        return Err(CliError::local(
            "Multiple skills target the same installation directory",
        ));
    }
    let lock_directory = lockfile.and_then(Path::parent).unwrap_or(cwd);
    let mut directories: Vec<PathBuf> = Vec::new();
    for directory in targets.iter().cloned().chain(
        lock.skills
            .iter()
            .map(|skill| resolve(lock_directory, &skill.path)),
    ) {
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    let physical_paths = directories
        .iter()
        .map(|directory| physical(directory))
        .collect::<Result<Vec<_>>>()?;
    if let Some(lockfile) = lockfile {
        let root = std::fs::canonicalize(cwd)
            .map_err(|error| CliError::local(format!("Cannot check {}: {error}", cwd.display())))?;
        let lock_path = physical(lockfile)?;
        for (directory, real) in directories.iter().zip(&physical_paths) {
            if contains(real, &lock_path) {
                return Err(CliError::local(format!(
                    "Lockfile must be outside skill directories: {}",
                    lockfile.display()
                )));
            }
            if selected.contains(directory) && (*real == root || !contains(&root, real)) {
                return Err(CliError::local(format!(
                    "Locked skill directory must stay within the project: {}. Use --no-lockfile to work \
                     with a directory outside the project.",
                    directory.display()
                )));
            }
        }
    }
    for i in 0..directories.len() {
        for j in i + 1..directories.len() {
            if !selected.contains(&directories[i]) && !selected.contains(&directories[j]) {
                continue;
            }
            if contains(&physical_paths[i], &physical_paths[j])
                || contains(&physical_paths[j], &physical_paths[i])
            {
                return Err(CliError::local(format!(
                    "Skill installation directories overlap: {} and {}",
                    directories[i].display(),
                    directories[j].display()
                )));
            }
        }
    }
    Ok(())
}
