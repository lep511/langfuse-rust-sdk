//! Comparing an installed skill directory with a manifest.

use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use crate::error::{CliError, Result};
use crate::skills::api::{SkillFile, sha256_base64};

/// UTF-8 text without leading byte order marks; `None` for invalid UTF-8.
pub fn decode_skill_content(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    Some(text.trim_start_matches('\u{feff}').to_owned())
}

fn matches_file(bytes: &[u8], expected: &SkillFile) -> bool {
    bytes.len() as u64 == expected.content_length && sha256_base64(bytes) == expected.sha256_hash
}

/// Whether `directory` is a real directory holding exactly the manifest's
/// files (regular files; content equal, or equal once a BOM is stripped).
pub fn matches_manifest(directory: &Path, files: &[SkillFile]) -> Result<bool> {
    match walk(directory, files) {
        Ok(matches) => Ok(matches),
        Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
            Ok(false)
        }
        Err(error) => Err(CliError::local(format!(
            "Cannot check installed skill at {}: {error}",
            directory.display()
        ))),
    }
}

fn walk(directory: &Path, files: &[SkillFile]) -> std::io::Result<bool> {
    let mut remaining: HashMap<&str, &SkillFile> = files
        .iter()
        .map(|file| (file.path.as_str(), file))
        .collect();
    if !std::fs::symlink_metadata(directory)?.is_dir() {
        return Ok(false);
    }
    let mut pending = vec![String::new()];
    while let Some(parent) = pending.pop() {
        for entry in std::fs::read_dir(directory.join(&parent))? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if parent.is_empty() {
                name
            } else {
                format!("{parent}/{name}")
            };
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            let Some(expected) = remaining.get(path.as_str()).copied() else {
                return Ok(false);
            };
            if !kind.is_file() {
                return Ok(false);
            }
            let bytes = std::fs::read(directory.join(&path))?;
            if !matches_file(&bytes, expected) {
                match decode_skill_content(&bytes) {
                    Some(content) if matches_file(content.as_bytes(), expected) => {}
                    _ => return Ok(false),
                }
            }
            remaining.remove(path.as_str());
        }
    }
    Ok(remaining.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, content: &str) -> SkillFile {
        SkillFile {
            path: path.into(),
            sha256_hash: sha256_base64(content.as_bytes()),
            content_length: content.len() as u64,
        }
    }

    #[test]
    fn manifest_matching() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("skill");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("SKILL.md"), "\u{feff}hello").unwrap();
        std::fs::write(root.join("docs/a.md"), "a").unwrap();
        let manifest = [file("SKILL.md", "hello"), file("docs/a.md", "a")];
        assert!(matches_manifest(&root, &manifest).unwrap());
        assert!(!matches_manifest(&root, &manifest[..1]).unwrap());
        assert!(
            !matches_manifest(&root, &[file("SKILL.md", "hello"), file("docs/a.md", "b")]).unwrap()
        );
        assert!(!matches_manifest(&dir.path().join("missing"), &manifest).unwrap());
        std::fs::create_dir(root.join("empty")).unwrap();
        assert!(matches_manifest(&root, &manifest).unwrap());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("docs/a.md"), root.join("link.md")).unwrap();
            assert!(
                !matches_manifest(
                    &root,
                    &[
                        file("SKILL.md", "hello"),
                        file("docs/a.md", "a"),
                        file("link.md", "a")
                    ]
                )
                .unwrap()
            );
        }
    }
}
