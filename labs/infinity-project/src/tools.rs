//! Tools that let the model look at the user's files. Read-only, and limited
//! to the directory the chat was started in.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use infinity_agent_core::system::local::ChannelSender;
use infinity_agent_core::tools::{Tool, ToolContext};
use infinity_provider_protocol::message::{Text, ToolResult, ToolResultContent};
use rap_protocol::{ProviderCallId, ToolCallId};
use serde_json::{Value, json};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A listing stops here, so that one huge directory cannot fill the model's
/// context.
const MAX_ENTRIES: usize = 500;

/// Lists a directory: names, with sizes for files. It never returns file
/// contents.
pub struct ListDirectoryTool {
    /// The working directory, canonical. Nothing outside it is listed.
    root: PathBuf,
}

impl ListDirectoryTool {
    /// A tool limited to the current directory.
    pub fn in_current_dir() -> io::Result<Self> {
        Ok(Self {
            root: std::env::current_dir()?.canonicalize()?,
        })
    }
}

#[async_trait]
impl Tool<ChannelSender> for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List the files and subdirectories of a directory on the user's machine. `path` is relative to the user's working directory, the one this chat was started in; leave it out to list the working directory itself. Only the working directory and what is below it can be listed, one directory per call. This is read-only and returns names and sizes, not file contents."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list, relative to the working directory. Defaults to the working directory."
                }
            },
            "required": []
        })
    }

    fn display_script(&self) -> Option<&str> {
        Some(r#""Listing " + if args.path == () { "." } else { args.path }"#)
    }

    /// The listing is ready within the round, so the model reads it without
    /// the thread going through its input queue.
    fn supports_sync(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        _args: Value,
        _id: ToolCallId,
        _call_id: Option<ProviderCallId>,
        _context: &ToolContext<ChannelSender>,
    ) -> Result<(), BoxError> {
        // Never called: `execute_synchronous` always answers.
        Ok(())
    }

    async fn execute_synchronous(
        &self,
        args: &Value,
        id: &ToolCallId<str>,
        call_id: Option<&ProviderCallId<str>>,
        _context: &ToolContext<ChannelSender>,
    ) -> Option<ToolResult> {
        let root = self.root.clone();
        let path = args.get("path").and_then(Value::as_str).map(str::to_owned);
        // Off the thread that runs the chat: a slow disk must not freeze it.
        let listing = tokio::task::spawn_blocking(move || list_directory(&root, path.as_deref()))
            .await
            .unwrap_or_else(|error| Err(format!("the listing did not finish: {error}")));
        let text = listing.unwrap_or_else(|error| format!("Error: {error}"));
        Some(ToolResult {
            id: id.as_str().to_owned(),
            call_id: call_id.map(|call_id| call_id.as_str().to_owned()),
            content: vec![ToolResultContent::Text(Text { text })],
        })
    }
}

/// Lists `path` (relative to `root`, which must be canonical; `root` itself
/// when `None`), one entry per line, directories first. The error is a
/// message for the model.
fn list_directory(root: &Path, path: Option<&str>) -> Result<String, String> {
    let requested = path.unwrap_or(".");
    // Canonical, so that neither `..` nor a symlink leads out of `root`.
    let dir = root
        .join(requested)
        .canonicalize()
        .map_err(|error| format!("cannot open '{requested}': {error}"))?;
    if !dir.starts_with(root) {
        return Err(format!(
            "'{requested}' is outside the working directory, which is all this tool can list"
        ));
    }

    // (is not a directory, name, description)
    let mut entries: Vec<(bool, String, String)> = fs::read_dir(&dir)
        .map_err(|error| format!("cannot list '{requested}': {error}"))?
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Neither call follows a symlink.
            match (entry.file_type(), entry.metadata()) {
                (Ok(kind), _) if kind.is_dir() => (false, name.clone(), format!("{name}/")),
                (Ok(kind), _) if kind.is_symlink() => {
                    (true, name.clone(), format!("{name}  (symlink)"))
                }
                (_, Ok(metadata)) => {
                    let size = metadata.len();
                    (true, name.clone(), format!("{name}  ({size} bytes)"))
                }
                _ => (true, name.clone(), name),
            }
        })
        .collect();
    entries.sort();

    let total = entries.len();
    let mut listing = format!("{} ({total} entries)", dir.display());
    for (_, _, description) in entries.iter().take(MAX_ENTRIES) {
        listing.push('\n');
        listing.push_str(description);
    }
    if total > MAX_ENTRIES {
        listing.push_str(&format!("\n… and {} more", total - MAX_ENTRIES));
    }
    Ok(listing)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{MAX_ENTRIES, list_directory};

    /// A directory under the system's temporary one, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("infinity-project-{}-{name}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path.canonicalize().unwrap())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn lists_directories_first_with_file_sizes() {
        let root = TempDir::new("lists");
        fs::create_dir(root.path().join("src")).unwrap();
        fs::write(root.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.path().join("Cargo.toml"), "[package]").unwrap();
        fs::write(root.path().join(".gitignore"), "").unwrap();

        assert_eq!(
            list_directory(root.path(), None).unwrap(),
            format!(
                "{} (3 entries)\nsrc/\n.gitignore  (0 bytes)\nCargo.toml  (9 bytes)",
                root.path().display()
            )
        );
        assert_eq!(
            list_directory(root.path(), Some("src")).unwrap(),
            format!(
                "{} (1 entries)\nmain.rs  (12 bytes)",
                root.path().join("src").display()
            )
        );
    }

    #[test]
    fn refuses_what_is_outside_the_root() {
        let outer = TempDir::new("outside");
        let root = outer.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::create_dir(outer.path().join("sibling")).unwrap();

        for path in ["..", "../sibling", "/", outer.path().to_str().unwrap()] {
            let error = list_directory(&root, Some(path)).unwrap_err();
            assert!(error.contains("outside the working directory"), "{error}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_leaves_the_root() {
        let outer = TempDir::new("symlink");
        let root = outer.path().join("root");
        fs::create_dir(&root).unwrap();
        fs::create_dir(outer.path().join("secret")).unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret"), root.join("link")).unwrap();

        let error = list_directory(&root, Some("link")).unwrap_err();
        assert!(error.contains("outside the working directory"), "{error}");
        // The link itself is listed, as a link.
        assert!(
            list_directory(&root, None)
                .unwrap()
                .ends_with("link  (symlink)")
        );
    }

    #[test]
    fn reports_a_missing_directory_and_a_file() {
        let root = TempDir::new("missing");
        fs::write(root.path().join("file.txt"), "x").unwrap();

        let error = list_directory(root.path(), Some("nope")).unwrap_err();
        assert!(error.starts_with("cannot open 'nope'"), "{error}");
        let error = list_directory(root.path(), Some("file.txt")).unwrap_err();
        assert!(error.starts_with("cannot list 'file.txt'"), "{error}");
    }

    #[test]
    fn stops_at_the_entry_limit() {
        let root = TempDir::new("limit");
        for i in 0..MAX_ENTRIES + 2 {
            fs::write(root.path().join(format!("f{i:04}")), "").unwrap();
        }

        let listing = list_directory(root.path(), None).unwrap();
        assert_eq!(listing.lines().count(), MAX_ENTRIES + 2);
        assert!(listing.ends_with("… and 2 more"), "{listing}");
    }
}
