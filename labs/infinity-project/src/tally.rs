//! How often each command has been run, kept from one chat to the next: the
//! menu of commands lists the most used first.
//!
//! The counts are in a file of the user's state directory, a line for each
//! command with its name and its count.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io, process};

use crate::command::{COMMANDS, Entry};

/// The chat's directory in the user's state directory, and its file there.
const STATE_PATH: [&str; 2] = ["infinity-project", "command-usage"];

type Counts = BTreeMap<String, u64>;

pub struct Tally {
    /// The file the counts are kept in, as long as they can be.
    file: Option<PathBuf>,
    counts: Counts,
}

impl Tally {
    /// The user's counts: the ones kept in `$XDG_STATE_HOME`, or in
    /// `~/.local/state` without it.
    pub fn of_user() -> Self {
        // A relative path is not one to use, says the XDG Base Directory
        // Specification.
        let state = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| Some(env::home_dir()?.join(".local").join("state")));
        match state {
            Some(state) => {
                Self::kept_in(STATE_PATH.iter().fold(state, |path, part| path.join(part)))
            }
            None => Self::unkept(),
        }
    }

    /// Counts that start at zero and end with the chat.
    pub fn unkept() -> Self {
        Self {
            file: None,
            counts: Counts::new(),
        }
    }

    /// The counts kept in `file`. One that cannot be read is found out
    /// about when the first command is counted.
    fn kept_in(file: PathBuf) -> Self {
        Self {
            counts: read(&file).unwrap_or_default(),
            file: Some(file),
        }
    }

    /// Counts a run of the command `name`.
    ///
    /// # Errors
    ///
    /// When the counts cannot be kept. They are counted on until the chat
    /// ends, and the file is left alone.
    pub fn count(&mut self, name: &str) -> io::Result<()> {
        let add = |counts: &mut Counts| {
            let count = counts.entry(name.to_owned()).or_default();
            *count = count.saturating_add(1);
        };
        let Some(file) = &self.file else {
            add(&mut self.counts);
            return Ok(());
        };
        // Read again: other chats may have counted since.
        let kept = read(file).and_then(|mut counts| {
            add(&mut counts);
            write(file, &counts)?;
            Ok(counts)
        });
        match kept {
            Ok(counts) => {
                self.counts = counts;
                Ok(())
            }
            Err(error) => {
                self.file = None;
                add(&mut self.counts);
                Err(error)
            }
        }
    }

    /// The commands, the most run first. Those run as often are in the
    /// order of [`COMMANDS`].
    pub fn ranking(&self) -> Vec<&'static Entry> {
        let mut commands: Vec<_> = COMMANDS.iter().collect();
        commands.sort_by_key(|entry| Reverse(self.counts.get(entry.name).copied().unwrap_or(0)));
        commands
    }
}

/// The counts in `file`: none when there is no such file. A line that is not
/// a name and a count is skipped.
fn read(file: &Path) -> io::Result<Counts> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Counts::new()),
        Err(error) => return Err(error),
    };
    let count = |line: &str| {
        let (name, count) = line.split_once(' ')?;
        Some((name.to_owned(), count.trim().parse().ok()?))
    };
    Ok(text.lines().filter_map(count).collect())
}

fn write(file: &Path, counts: &Counts) -> io::Result<()> {
    if let Some(directory) = file.parent() {
        fs::create_dir_all(directory)?;
    }
    let text: String = counts
        .iter()
        .map(|(name, count)| format!("{name} {count}\n"))
        .collect();
    // Written aside and moved into place, so that a chat that reads the
    // file meanwhile finds all of it.
    let aside = file.with_extension(format!("{}.tmp", process::id()));
    fs::write(&aside, text)
        .and_then(|()| fs::rename(&aside, file))
        .inspect_err(|_| {
            let _ = fs::remove_file(&aside);
        })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::Tally;

    /// A path under the system's temporary directory, removed on drop with
    /// what is in it.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("infinity-project-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }

        /// The file of the counts, in a directory that does not exist yet.
        fn file(&self) -> PathBuf {
            self.0.join("state").join("command-usage")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn names(tally: &Tally) -> Vec<&'static str> {
        tally.ranking().iter().map(|entry| entry.name).collect()
    }

    #[test]
    fn commands_never_run_are_in_their_own_order() {
        assert_eq!(
            names(&Tally::unkept()),
            ["clear", "usage", "model", "exit", "help"]
        );
    }

    #[test]
    fn the_most_run_commands_come_first() {
        let mut tally = Tally::unkept();
        for name in ["help", "exit", "help", "usage", "exit", "help"] {
            tally.count(name).unwrap();
        }
        assert_eq!(names(&tally), ["help", "exit", "usage", "clear", "model"]);
    }

    #[test]
    fn counts_are_kept_from_one_chat_to_the_next() {
        let dir = TempDir::new("kept");
        let mut first = Tally::kept_in(dir.file());
        assert_eq!(names(&first), ["clear", "usage", "model", "exit", "help"]);
        first.count("exit").unwrap();
        first.count("usage").unwrap();
        first.count("exit").unwrap();
        assert_eq!(fs::read_to_string(dir.file()).unwrap(), "exit 2\nusage 1\n");

        let second = Tally::kept_in(dir.file());
        assert_eq!(names(&second), ["exit", "usage", "clear", "model", "help"]);
        // Nothing else is left in the directory.
        let left = fs::read_dir(dir.file().parent().unwrap()).unwrap();
        assert_eq!(left.count(), 1);
    }

    #[test]
    fn chats_at_the_same_time_count_together() {
        let dir = TempDir::new("together");
        let mut one = Tally::kept_in(dir.file());
        let mut other = Tally::kept_in(dir.file());
        one.count("help").unwrap();
        other.count("help").unwrap();
        other.count("usage").unwrap();
        one.count("usage").unwrap();
        one.count("usage").unwrap();
        assert_eq!(fs::read_to_string(dir.file()).unwrap(), "help 2\nusage 3\n");
        assert_eq!(names(&one), ["usage", "help", "clear", "model", "exit"]);
    }

    #[test]
    fn lines_that_are_not_counts_are_skipped() {
        let dir = TempDir::new("skipped");
        fs::create_dir_all(dir.file().parent().unwrap()).unwrap();
        let text = "help 3\n\nexit\nusage many\nclear 18446744073709551615\ngone 7\n";
        fs::write(dir.file(), text).unwrap();
        let mut tally = Tally::kept_in(dir.file());
        assert_eq!(names(&tally), ["clear", "help", "usage", "model", "exit"]);
        // The largest count stays where it is, and a command the chat no
        // longer has keeps its count.
        tally.count("clear").unwrap();
        assert_eq!(
            fs::read_to_string(dir.file()).unwrap(),
            "clear 18446744073709551615\ngone 7\nhelp 3\n"
        );
    }

    #[test]
    fn counts_that_cannot_be_kept_last_as_long_as_the_chat() {
        let dir = TempDir::new("unkept");
        // The directory of the file is a file.
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("state"), "").unwrap();
        let mut tally = Tally::kept_in(dir.file());
        assert!(tally.count("help").is_err());
        // Said once: the counts go on without the file.
        tally.count("help").unwrap();
        tally.count("usage").unwrap();
        assert_eq!(names(&tally), ["help", "usage", "clear", "model", "exit"]);
        assert_eq!(fs::read_to_string(dir.0.join("state")).unwrap(), "");
    }
}
