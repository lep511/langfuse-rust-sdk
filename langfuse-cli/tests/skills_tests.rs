//! `langfuse skills install|update|push` against a fake skills API (ports of
//! the official CLI's skills tests).

mod common;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use langfuse_rust_cli::error::Exit;
use serde_json::{Value, json};
use sha2::Digest as _;
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn hash(content: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(content.as_bytes()))
}

#[derive(Default)]
struct State {
    /// name → versions (manifests without content).
    versions: HashMap<String, Vec<Value>>,
    /// hash → content.
    contents: HashMap<String, String>,
    /// Override of the contents returned for a hash (corruption tests).
    corrupt: HashMap<String, String>,
    tags: HashMap<String, Vec<String>>,
}

#[derive(Clone, Default)]
struct FakeSkills(Arc<Mutex<State>>);

fn manifest(name: &str, version: i64, labels: &[&str], files: &[(&str, &str)]) -> Value {
    json!({
        "id": format!("{name}-{version}"),
        "createdAt": "2026-11-01T00:00:00Z",
        "updatedAt": "2026-11-01T00:00:00Z",
        "projectId": "p",
        "createdBy": "API",
        "name": name,
        "description": "d",
        "frontmatter": null,
        "version": version,
        "tags": [],
        "labels": labels,
        "commitMessage": null,
        "files": files.iter().map(|(path, content)| json!({
            "id": format!("f-{path}"),
            "path": path,
            "blobId": "b",
            "sha256Hash": hash(content),
            "contentType": "text/markdown",
            "contentLength": content.len(),
        })).collect::<Vec<_>>(),
    })
}

impl FakeSkills {
    fn add(&self, name: &str, version: i64, labels: &[&str], files: &[(&str, &str)]) {
        let mut state = self.0.lock().unwrap();
        for (_, content) in files {
            state.contents.insert(hash(content), (*content).to_owned());
        }
        state
            .versions
            .entry(name.to_owned())
            .or_default()
            .push(manifest(name, version, labels, files));
    }

    fn tag(&self, name: &str, tag: &str) {
        self.0
            .lock()
            .unwrap()
            .tags
            .entry(name.to_owned())
            .or_default()
            .push(tag.to_owned());
    }

    fn corrupt(&self, content: &str, replacement: &str) {
        self.0
            .lock()
            .unwrap()
            .corrupt
            .insert(hash(content), replacement.to_owned());
    }

    async fn mount(&self, server: &MockServer) {
        Mock::given(wiremock::matchers::any())
            .respond_with(self.clone())
            .mount(server)
            .await;
    }
}

fn not_found() -> ResponseTemplate {
    ResponseTemplate::new(404)
        .set_body_json(json!({"message": "Skill not found", "error": "LangfuseNotFoundError"}))
}

impl Respond for FakeSkills {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut state = self.0.lock().unwrap();
        let query: HashMap<String, String> = request.url.query_pairs().into_owned().collect();
        let segments: Vec<&str> = request
            .url
            .path()
            .trim_start_matches("/api/public/skills")
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        match (request.method.as_str(), segments.as_slice()) {
            ("GET", ["files", "content"]) => {
                let data: Vec<Value> = query["sha256Hashes"]
                    .split(',')
                    .map(|hash| {
                        let content = state
                            .corrupt
                            .get(hash)
                            .or_else(|| state.contents.get(hash))
                            .cloned()
                            .unwrap_or_default();
                        json!({"sha256Hash": hash, "content": content})
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({"data": data}))
            }
            ("GET", []) => {
                let tag = query.get("tag");
                let mut names: Vec<&String> = state.versions.keys().collect();
                names.sort();
                let data: Vec<Value> = names
                    .into_iter()
                    .filter(|name| tag.is_none_or(|tag| state.tags.get(*name).is_some_and(|tags| tags.contains(tag))))
                    .map(|name| {
                        let versions = &state.versions[name];
                        let production = versions
                            .iter()
                            .find(|v| v["labels"].as_array().unwrap().iter().any(|l| l == "production"))
                            .map(|v| v["version"].clone())
                            .unwrap_or(Value::Null);
                        json!({
                            "name": name, "description": null, "tags": [], "latestVersion": versions.len(),
                            "latestVersionCreatedAt": "2026-11-01T00:00:00Z",
                            "latestVersionLastUpdatedAt": "2026-11-01T00:00:00Z",
                            "productionVersion": production,
                        })
                    })
                    .collect();
                ResponseTemplate::new(200).set_body_json(json!({
                    "data": data,
                    "meta": {"page": 1, "limit": 100, "totalItems": 1, "totalPages": 1, "hasNextPage": false},
                }))
            }
            ("GET", [name]) => {
                let Some(versions) = state.versions.get(*name) else {
                    return not_found();
                };
                let found = match (query.get("version"), query.get("label")) {
                    (Some(version), _) => versions
                        .iter()
                        .find(|v| v["version"].as_i64() == version.parse::<i64>().ok()),
                    (None, Some(label)) => versions
                        .iter()
                        .find(|v| v["labels"].as_array().unwrap().iter().any(|l| l == label)),
                    _ => None,
                };
                match found {
                    Some(manifest) => ResponseTemplate::new(200).set_body_json(manifest),
                    None => not_found(),
                }
            }
            ("POST", []) => {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let files: Vec<(String, String)> = body["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| {
                        (
                            f["path"].as_str().unwrap().to_owned(),
                            f["content"].as_str().unwrap().to_owned(),
                        )
                    })
                    .collect();
                let skill = &files.iter().find(|(path, _)| path == "SKILL.md").unwrap().1;
                let name = skill
                    .lines()
                    .find_map(|line| line.strip_prefix("name: "))
                    .unwrap()
                    .trim()
                    .to_owned();
                for (_, content) in &files {
                    state.contents.insert(hash(content), content.clone());
                }
                let versions = state.versions.entry(name.clone()).or_default();
                for version in versions.iter_mut() {
                    let labels: Vec<Value> = version["labels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|l| *l != "latest")
                        .cloned()
                        .collect();
                    version["labels"] = Value::Array(labels);
                }
                let refs: Vec<(&str, &str)> = files
                    .iter()
                    .map(|(p, c)| (p.as_str(), c.as_str()))
                    .collect();
                let created = manifest(&name, versions.len() as i64 + 1, &["latest"], &refs);
                versions.push(created.clone());
                ResponseTemplate::new(201).set_body_json(created)
            }
            ("PATCH", [name, "versions", version]) => {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let labels = body["labels"].as_array().unwrap().clone();
                let Some(versions) = state.versions.get_mut(*name) else {
                    return not_found();
                };
                for v in versions.iter_mut() {
                    let kept: Vec<Value> = v["labels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|l| !labels.contains(l))
                        .cloned()
                        .collect();
                    v["labels"] = Value::Array(kept);
                }
                let Some(target) = versions
                    .iter_mut()
                    .find(|v| v["version"].as_i64() == version.parse::<i64>().ok())
                else {
                    return not_found();
                };
                let mut merged = target["labels"].as_array().unwrap().clone();
                merged.extend(labels);
                target["labels"] = Value::Array(merged);
                ResponseTemplate::new(200).set_body_json(target.clone())
            }
            _ => ResponseTemplate::new(500),
        }
    }
}

struct Project {
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
    server: MockServer,
    fake: FakeSkills,
}

impl Project {
    async fn new() -> Self {
        let (dir, root) = common::temp_dir();
        let server = MockServer::start().await;
        let fake = FakeSkills::default();
        fake.mount(&server).await;
        Self {
            _dir: dir,
            root,
            server,
            fake,
        }
    }

    async fn skills(&self, args: &[&str]) -> common::Output {
        self.run(&[&["--json", "skills"], args].concat()).await
    }

    async fn run(&self, args: &[&str]) -> common::Output {
        let connection = common::connection(&self.server);
        let mut all: Vec<&str> = connection.iter().map(String::as_str).collect();
        all.extend_from_slice(args);
        common::run_with(&self.root, b"", &all).await
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.root.join(path)).unwrap()
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn lock(&self) -> Value {
        serde_json::from_str(&self.read(".agents/skills/langfuse-skills.lock")).unwrap()
    }

    async fn requests(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

const SKILL: &str = "---\nname: example\ndescription: An example\n---\nDo things.\n";

#[tokio::test]
async fn install_writes_files_and_the_lockfile() {
    let project = Project::new().await;
    project.fake.add(
        "example",
        1,
        &["production"],
        &[("SKILL.md", SKILL), ("docs/guide.md", "guide")],
    );

    let output = project.skills(&["install", "example"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let result = output.json();
    assert_eq!(result["status"], "installed");
    assert_eq!(result["version"], 1);
    assert_eq!(result["files"], 2);
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), SKILL);
    assert_eq!(
        project.read(".agents/skills/example/docs/guide.md"),
        "guide"
    );
    assert_eq!(
        project.lock(),
        json!({"lockVersion": 1, "skills": [{"path": "example", "name": "example", "selector": {"label": "production"}, "version": 1}]})
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(project.root.join(".agents/skills/example/SKILL.md"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o644);
    }

    let output = project.skills(&["install", "example"]).await;
    assert_eq!(output.json()["status"], "unchanged");

    // Text output.
    let output = project.run(&["skills", "install", "example"]).await;
    assert!(
        output
            .stdout
            .starts_with("Already up to date: example@1 at "),
        "{}",
        output.stdout
    );
}

#[tokio::test]
async fn selectors_must_match_the_returned_manifest() {
    let project = Project::new().await;
    project
        .fake
        .add("example", 1, &["production"], &[("SKILL.md", SKILL)]);
    project
        .fake
        .add("example", 2, &["staging"], &[("SKILL.md", "v2")]);

    let output = project.skills(&["install", "example@staging"]).await;
    assert_eq!(output.json()["version"], 2);
    assert_eq!(
        project.lock()["skills"][0]["selector"],
        json!({"label": "staging"})
    );

    let output = project.skills(&["install", "example@1", "--force"]).await;
    assert_eq!(output.json()["version"], 1);
    assert_eq!(
        project.lock()["skills"][0]["selector"],
        json!({"version": 1})
    );

    // The server answers a label request with a version that lacks the label.
    let project = Project::new().await;
    let lying = manifest("example", 1, &["production"], &[("SKILL.md", SKILL)]);
    Mock::given(wiremock::matchers::path("/api/public/skills/example"))
        .respond_with(ResponseTemplate::new(200).set_body_json(lying))
        .with_priority(1)
        .mount(&project.server)
        .await;
    let output = project.skills(&["install", "example@staging"]).await;
    assert_eq!(output.exit, Exit::Response, "{output:?}");
    assert_eq!(
        output.stderr,
        "Requested example@staging; server returned labels [\"production\"]\n"
    );
    assert!(
        !project
            .root
            .join(".agents/skills/langfuse-skills.lock")
            .exists()
    );
}

#[tokio::test]
async fn checksum_mismatches_keep_the_installed_copy() {
    let project = Project::new().await;
    project
        .fake
        .add("example", 1, &["production"], &[("SKILL.md", SKILL)]);
    project.skills(&["install", "example"]).await;
    project
        .fake
        .add("example", 2, &["staging"], &[("SKILL.md", "v2 content")]);
    project.fake.corrupt("v2 content", "v2 conteNt");

    let output = project
        .skills(&["install", "example@staging", "--force"])
        .await;
    assert_eq!(output.exit, Exit::Response, "{output:?}");
    assert_eq!(output.stderr, "Downloaded checksum mismatch for SKILL.md\n");
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), SKILL);
    let leftovers: Vec<_> = std::fs::read_dir(project.root.join(".agents/skills"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers
            .iter()
            .all(|name| !name.contains(".install-") && !name.contains(".backup-")),
        "{leftovers:?}"
    );
}

#[tokio::test]
async fn local_changes_need_force_and_updates_replace_clean_copies() {
    let project = Project::new().await;
    project
        .fake
        .add("example", 1, &["production"], &[("SKILL.md", SKILL)]);
    project.skills(&["install", "example"]).await;

    // A new production version: a clean copy of v1 is updated.
    project.fake.add("example", 2, &[], &[("SKILL.md", "v2")]);
    {
        let mut state = project.fake.0.lock().unwrap();
        let versions = state.versions.get_mut("example").unwrap();
        versions[0]["labels"] = json!([]);
        versions[1]["labels"] = json!(["production"]);
    }
    let output = project.skills(&["update"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(output.json()[0]["version"], 2);
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), "v2");
    assert_eq!(project.lock()["skills"][0]["version"], 2);

    // Local edits are preserved unless --force.
    project.write(".agents/skills/example/SKILL.md", "edited");
    let output = project.skills(&["install", "example@1"]).await;
    assert_eq!(output.exit, Exit::Local, "{output:?}");
    assert!(
        output
            .stderr
            .ends_with("contains different files; pass --force to replace it\n")
    );
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), "edited");
    let output = project.skills(&["install", "example@1", "--force"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), SKILL);

    // A bare install restores the pinned versions.
    std::fs::remove_dir_all(project.root.join(".agents/skills/example")).unwrap();
    let output = project.skills(&["install"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(project.read(".agents/skills/example/SKILL.md"), SKILL);
}

#[tokio::test]
async fn tag_and_all_skip_skills_without_production() {
    let project = Project::new().await;
    project
        .fake
        .add("alpha", 1, &["production"], &[("SKILL.md", "a")]);
    project
        .fake
        .add("beta", 1, &["staging"], &[("SKILL.md", "b")]);
    project.fake.tag("alpha", "support");
    project.fake.tag("beta", "support");

    let output = project.skills(&["install", "--tag", "support"]).await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let results = output.json();
    assert_eq!(
        results[0],
        json!({"name": "beta", "status": "skipped", "reason": "no-production-version"})
    );
    assert_eq!(results[1]["name"], "alpha");
    assert_eq!(output.stderr, "Skipped beta: no production version\n");

    let output = project.skills(&["install", "--all", "--no-lockfile"]).await;
    let results = output.json();
    assert_eq!(results.as_array().unwrap().len(), 2, "{results}");
    assert_eq!(results[1]["status"], "unchanged");

    // With --label every selected skill must have it.
    let output = project
        .skills(&["install", "--all", "--label", "staging", "--no-lockfile"])
        .await;
    assert_eq!(output.exit, Exit::Http, "{output:?}");
    assert!(
        output
            .stderr
            .contains("/api/public/skills/alpha?label=staging returned HTTP 404: Skill not found")
    );

    let output = project.run(&["skills", "install", "--tag", "none"]).await;
    assert_eq!(output.stdout, "No skills found with tag \"none\"\n");
}

#[tokio::test]
async fn locked_directories_stay_inside_the_project() {
    let project = Project::new().await;
    project
        .fake
        .add("example", 1, &["production"], &[("SKILL.md", SKILL)]);
    let outside = common::temp_dir();
    for directory in [outside.1.to_str().unwrap(), ".."] {
        let output = project
            .skills(&["install", "example", "--directory", directory])
            .await;
        assert_eq!(output.exit, Exit::Local, "{directory}: {output:?}");
        assert!(
            output.stderr.contains("must stay within the project"),
            "{}",
            output.stderr
        );
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside.1, project.root.join("link")).unwrap();
        let output = project
            .skills(&["install", "example", "--directory", "link"])
            .await;
        assert_eq!(output.exit, Exit::Local, "{output:?}");
    }
    assert_eq!(project.requests().await, 0);

    // Without a lockfile any directory works.
    let output = project
        .skills(&[
            "install",
            "example",
            "--directory",
            outside.1.to_str().unwrap(),
            "--no-lockfile",
        ])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
}

#[tokio::test]
async fn unsafe_manifests_are_rejected() {
    let project = Project::new().await;
    project.fake.add(
        "example",
        1,
        &["production"],
        &[("SKILL.md", SKILL), ("../escape.md", "x")],
    );
    let output = project.skills(&["install", "example"]).await;
    assert_eq!(output.exit, Exit::Response, "{output:?}");
    assert_eq!(
        output.stderr,
        "Skill contains unsafe file path: \"../escape.md\"\n"
    );

    let project = Project::new().await;
    project
        .fake
        .add("example", 1, &["production"], &[("README.md", "x")]);
    let output = project.skills(&["install", "example"]).await;
    assert_eq!(output.exit, Exit::Response);
    assert_eq!(
        output.stderr,
        "Langfuse skill manifest has no root SKILL.md\n"
    );
}

#[tokio::test]
async fn lockfile_errors() {
    let project = Project::new().await;
    let output = project.skills(&["update"]).await;
    assert_eq!(output.exit, Exit::Usage);
    assert!(
        output.stderr.starts_with("No skills recorded in "),
        "{}",
        output.stderr
    );

    project.write(
        ".agents/skills/langfuse-skills.lock",
        "{\"lockVersion\":2,\"skills\":[]}",
    );
    let output = project.skills(&["install"]).await;
    assert_eq!(output.exit, Exit::Local);
    assert!(
        output.stderr.contains("lockVersion must be 1"),
        "{}",
        output.stderr
    );
}

#[tokio::test]
async fn push_publishes_labels_and_records() {
    let project = Project::new().await;
    project.write("skills/example/SKILL.md", SKILL);
    project.write("skills/example/docs/a.md", "a");

    let output = project
        .skills(&["push", "skills/example", "--dry-run"])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let dry = output.json();
    assert_eq!(dry["status"], "would-create");
    assert_eq!(dry["version"], Value::Null);
    assert_eq!(dry["expectedVersion"], 1);
    assert_eq!(dry["included"], json!(["SKILL.md", "docs/a.md"]));
    assert_eq!(dry["changes"]["added"], json!(["SKILL.md", "docs/a.md"]));

    let output = project
        .skills(&[
            "push",
            "skills/example",
            "--label",
            "production",
            "--commit-message",
            "first",
        ])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let pushed = output.json();
    assert_eq!(pushed["status"], "created");
    assert_eq!(pushed["version"], 1);
    assert!(
        pushed["labels"]
            .as_array()
            .unwrap()
            .contains(&json!("production"))
    );
    let lock: Value = serde_json::from_str(&project.read("skills/langfuse-skills.lock")).unwrap();
    assert_eq!(
        lock["skills"][0],
        json!({"path": "example", "name": "example", "selector": {"label": "production"}, "version": 1})
    );

    let output = project.skills(&["push", "skills/example"]).await;
    assert_eq!(output.json()["status"], "unchanged");

    let output = project
        .run(&["skills", "push", "skills/example", "--label", "staging"])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert!(
        output
            .stdout
            .starts_with("Labeled: example@1 (2 files; labels: "),
        "{}",
        output.stdout
    );

    project.write("skills/example/docs/a.md", "changed");
    let output = project
        .run(&["skills", "push", "skills/example", "--dry-run"])
        .await;
    assert!(
        output.stdout.starts_with(
            "Dry run: example v1->v2 (0 added, 1 changed, 0 removed)\n  Include \"SKILL.md\""
        ),
        "{}",
        output.stdout
    );
}

#[tokio::test]
async fn recursive_push_skips_invalid_skills_and_update_only() {
    let project = Project::new().await;
    project.write("skills/a/SKILL.md", "---\nname: a\ndescription: A\n---\n");
    project.write("skills/broken/SKILL.md", "no frontmatter");
    project.write(
        "skills/nested/b/SKILL.md",
        "---\nname: b\ndescription: B\n---\n",
    );
    project
        .fake
        .add("b", 1, &["production", "latest"], &[("SKILL.md", "old")]);

    let output = project
        .skills(&[
            "push",
            "skills",
            "--recursive",
            "--update-only",
            "--dry-run",
        ])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let results = output.json();
    let results = results.as_array().unwrap();
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(results[0]["status"], "skipped");
    assert_eq!(results[0]["reason"], "invalid-skill");
    assert!(
        results[0]["message"]
            .as_str()
            .unwrap()
            .contains("SKILL.md must start with YAML frontmatter")
    );
    assert_eq!(results[1]["name"], "a");
    assert_eq!(results[1]["reason"], "not-found");
    assert_eq!(results[2]["name"], "b");
    assert_eq!(results[2]["status"], "would-create");
    assert_eq!(results[2]["expectedVersion"], 2);
}

#[tokio::test]
async fn push_validation() {
    let project = Project::new().await;
    project.write("skills/example/SKILL.md", SKILL);
    project.write("skills/example/run.exe", "binary");
    let output = project.skills(&["push", "skills/example"]).await;
    assert_eq!(output.exit, Exit::Local, "{output:?}");
    assert!(
        output
            .stderr
            .contains("Unsupported text file extension: run.exe"),
        "{}",
        output.stderr
    );

    let output = project
        .skills(&[
            "push",
            "skills/example",
            "--exclude",
            "run.exe",
            "--dry-run",
        ])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    assert_eq!(
        output.json()["excluded"],
        json!([{"path": "run.exe", "reason": "exclude"}])
    );

    let output = project
        .skills(&["push", "skills/example", "--exclude", "SKILL.md"])
        .await;
    assert_eq!(output.exit, Exit::Local);
    let before = project.requests().await;
    for (args, message) in [
        (
            vec!["push", "skills/example", "--label", "latest"],
            "--label must be",
        ),
        (
            vec!["push", "skills/example", "--exclude", "../x"],
            "--exclude requires",
        ),
        (
            vec!["push", "skills/example", "--lockfile", "x", "--no-lockfile"],
            "Use either --lockfile or --no-lockfile",
        ),
    ] {
        let output = project.skills(&args).await;
        assert_eq!(output.exit, Exit::Usage, "{args:?}");
        assert!(
            output.stderr.starts_with(message),
            "{args:?}: {}",
            output.stderr
        );
    }
    assert_eq!(project.requests().await, before);
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[tokio::test]
async fn push_honours_git_ignore_rules() {
    let project = Project::new().await;
    git(&project.root, &["init", "-q"]);
    project.write(".gitignore", "secret.json\nignored/\ntracked.json\n");
    project.write("skills/example/SKILL.md", SKILL);
    project.write("skills/example/secret.json", "{}");
    project.write("skills/example/ignored/x.md", "x");
    project.write("skills/example/tracked.json", "{}");
    git(&project.root, &["add", "-f", "skills/example/tracked.json"]);

    let output = project
        .skills(&["push", "skills/example", "--dry-run"])
        .await;
    assert_eq!(output.exit, Exit::Success, "{output:?}");
    let result = output.json();
    assert_eq!(result["included"], json!(["SKILL.md", "tracked.json"]));
    assert_eq!(
        result["excluded"],
        json!([{"path": "ignored/", "reason": "gitignore"}, {"path": "secret.json", "reason": "gitignore"}])
    );
}

#[tokio::test]
async fn control_characters_are_escaped() {
    let project = Project::new().await;
    project.write("skills/example/SKILL.md", SKILL);
    #[cfg(unix)]
    project.write("skills/example/a\u{9b}\x1b]52;c;x\x07.md", "x");
    let output = project
        .run(&["skills", "push", "skills/example", "--dry-run"])
        .await;
    #[cfg(unix)]
    {
        assert_eq!(output.exit, Exit::Local, "{output:?}");
        assert!(
            !output.stderr.contains('\x1b') && !output.stderr.contains('\u{9b}'),
            "{:?}",
            output.stderr
        );
        assert!(output.stderr.contains("\\u001b"), "{:?}", output.stderr);
    }
    let _ = output;
}

#[tokio::test]
async fn get_skill_prints_the_file_or_explains_the_failure() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::path("/SKILL.md"))
        .respond_with(ResponseTemplate::new(200).set_body_string("# Langfuse\n"))
        .mount(&server)
        .await;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin: &[u8] = b"";
    let mut io = langfuse_rust_cli::output::Io {
        stdout: &mut stdout,
        stderr: &mut stderr,
        stdin: &mut stdin,
        stdout_tty: false,
        stderr_tty: false,
        width: None,
        cwd: std::env::current_dir().unwrap(),
    };
    let exit =
        langfuse_rust_cli::get_skill::run_from(&format!("{}/SKILL.md", server.uri()), &mut io)
            .await
            .unwrap();
    assert_eq!(exit, Exit::Success);
    let exit =
        langfuse_rust_cli::get_skill::run_from(&format!("{}/missing.md", server.uri()), &mut io)
            .await
            .unwrap();
    assert_eq!(exit, Exit::Network);
    assert_eq!(String::from_utf8(stdout).unwrap(), "# Langfuse\n");
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(
        stderr.starts_with("Failed to fetch the latest Langfuse skill from GitHub."),
        "{stderr}"
    );
    assert!(
        stderr.contains("Original error: Failed to fetch skill from"),
        "{stderr}"
    );
}
