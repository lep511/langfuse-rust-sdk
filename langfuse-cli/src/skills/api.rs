//! The Langfuse skills API as `skills` commands use it: the SDK's typed
//! `client.skills` methods, plus the official CLI's validation of every
//! response (exit 7 when a manifest does not match what was requested).

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use futures_util::stream::{self, StreamExt as _, TryStreamExt as _};
use langfuse_rust_sdk::api::skills::{
    GetSkillParams, ListSkillsParams, SetSkillVersionLabelsRequest,
};
use langfuse_rust_sdk::models::skills::{
    CreateSkillVersionRequest, SkillVersion as SdkSkillVersion, SkillVersionFileCreateInput,
};
use langfuse_rust_sdk::{LangfuseClient, LangfuseError};
use serde_json::Value;

use crate::config::Settings;
use crate::error::{CliError, Exit, Result, error_chain};
use crate::skills::lock::Selector;
use crate::skills::types::{SkillName, Version};

/// Hashes per file-content request.
pub const FILE_CONTENT_BATCH_SIZE: usize = 50;
/// Pages of the skill list read before giving up.
const MAX_SKILL_LIST_PAGES: i64 = 100;
/// Requests of one kind in flight at once (manifests, file contents).
pub const REQUEST_CONCURRENCY: usize = 4;

const INVALID_MANIFEST: &str = "Langfuse returned an invalid skill manifest";
const INVALID_LIST: &str = "Langfuse returned an invalid skill list";
const INVALID_CONTENTS: &str = "Langfuse returned invalid skill file contents";

/// A canonical base64 encoding of 32 bytes.
pub fn is_sha256(value: &str) -> bool {
    STANDARD
        .decode(value)
        .is_ok_and(|bytes| bytes.len() == 32 && STANDARD.encode(&bytes) == value)
}

/// The base64 SHA-256 of `bytes`, as in skill manifests.
pub fn sha256_base64(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    STANDARD.encode(sha2::Sha256::digest(bytes))
}

/// One file of a validated manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFile {
    /// `/`-separated path.
    pub path: String,
    /// Base64 SHA-256 of the content.
    pub sha256_hash: String,
    /// UTF-8 byte length.
    pub content_length: u64,
}

/// A validated skill version.
#[derive(Debug, Clone)]
pub struct SkillVersion {
    /// Skill name.
    pub name: SkillName,
    /// Version number.
    pub version: Version,
    /// Labels of the version.
    pub labels: Vec<String>,
    /// Manifest.
    pub files: Vec<SkillFile>,
}

/// One entry of the skill list.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    /// Skill name.
    pub name: SkillName,
    /// The version labelled `production`, if any.
    pub production_version: Option<Version>,
}

/// The skills API of one connection.
///
/// Its methods take `&self`, so requests can run concurrently; the client is
/// built once, at the first request.
pub struct Remote {
    settings: Settings,
    client: OnceCell<LangfuseClient>,
}

impl std::fmt::Debug for Remote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The settings hold the secret key.
        f.debug_struct("Remote")
            .field("host", &self.settings.host)
            .finish_non_exhaustive()
    }
}

fn invalid(message: &str) -> CliError {
    CliError::response(message)
}

/// A rejected request for one skill version, kept apart from [`CliError`] so
/// that "this skill does not exist" can be told from a failure.
struct Rejected {
    path: String,
    error: Box<LangfuseError>,
}

impl Remote {
    /// A connection; credentials are checked at the first request.
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            client: OnceCell::new(),
        }
    }

    fn client(&self) -> Result<&LangfuseClient> {
        self.settings.require_credentials()?;
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let client = self.settings.client()?;
        Ok(self.client.get_or_init(|| client))
    }

    /// The error of a failed request: HTTP statuses are exit 5 with the
    /// server's message, transport failures exit 4, undecodable responses 7.
    fn failure(
        &self,
        method: &str,
        path: &str,
        error: LangfuseError,
        invalid_message: &str,
    ) -> CliError {
        let url = format!("{}{path}", self.settings.host);
        match &error {
            LangfuseError::Transport(inner) => CliError::new(
                Exit::Network,
                format!("{method} {url} failed: {}", error_chain(inner)),
            ),
            LangfuseError::Serialization(_) => invalid(invalid_message),
            _ => match error.status() {
                Some(status) => {
                    let message = error
                        .message()
                        .map(|message| format!(": {message}"))
                        .unwrap_or_default();
                    CliError::new(
                        Exit::Http,
                        format!("{method} {url} returned HTTP {status}{message}"),
                    )
                }
                None => error.into(),
            },
        }
    }

    /// Every skill (optionally with `tag`), deduplicated by name.
    pub async fn fetch_skills(&self, tag: Option<&str>) -> Result<Vec<SkillSummary>> {
        let client = self.client()?;
        let mut order: Vec<SkillName> = Vec::new();
        let mut skills: HashMap<SkillName, SkillSummary> = HashMap::new();
        for page in 1..=MAX_SKILL_LIST_PAGES {
            let params = ListSkillsParams {
                page: Some(page),
                limit: Some(100),
                tag: tag.map(str::to_owned),
                ..ListSkillsParams::default()
            };
            let response = client.skills.list(&params).await.map_err(|error| {
                let path = format!(
                    "/api/public/skills?{}",
                    serde_urlencoded::to_string(&params).unwrap_or_default()
                );
                self.failure("GET", &path, error, INVALID_LIST)
            })?;
            for skill in response.data {
                let name = SkillName::new(skill.name).ok_or_else(|| invalid(INVALID_LIST))?;
                let production_version = skill
                    .production_version
                    .map(|version| Version::new(version).ok_or_else(|| invalid(INVALID_LIST)))
                    .transpose()?;
                if !skills.contains_key(&name) {
                    order.push(name.clone());
                }
                skills.insert(
                    name.clone(),
                    SkillSummary {
                        name,
                        production_version,
                    },
                );
            }
            if !response.meta.has_next_page {
                return Ok(order
                    .into_iter()
                    .filter_map(|name| skills.remove(&name))
                    .collect());
            }
        }
        Err(invalid(&format!(
            "Langfuse skill listing exceeded {MAX_SKILL_LIST_PAGES} pages; no skills were installed"
        )))
    }

    async fn get(
        &self,
        name: &SkillName,
        selector: &Selector,
    ) -> Result<std::result::Result<SdkSkillVersion, Rejected>> {
        let client = self.client()?;
        let params = match selector {
            Selector::Label(label) => GetSkillParams {
                label: Some(label.clone()),
                ..GetSkillParams::default()
            },
            Selector::Version(version) => GetSkillParams {
                version: Some(version.get()),
                ..GetSkillParams::default()
            },
        };
        Ok(client
            .skills
            .get(name.as_str(), &params)
            .await
            .map_err(|error| Rejected {
                path: format!(
                    "/api/public/skills/{}?{}",
                    crate::operation::encode_segment(name.as_str()),
                    serde_urlencoded::to_string(&params).unwrap_or_default()
                ),
                error: Box::new(error),
            }))
    }

    fn rejected(&self, rejected: Rejected) -> CliError {
        self.failure("GET", &rejected.path, *rejected.error, INVALID_MANIFEST)
    }

    /// The version `selector` names; exit 7 unless it matches.
    pub async fn fetch_skill(&self, name: &SkillName, selector: &Selector) -> Result<SkillVersion> {
        match self.get(name, selector).await? {
            Ok(skill) => parse_skill_version(skill, name, Some(selector)),
            Err(rejected) => Err(self.rejected(rejected)),
        }
    }

    /// The `latest` version, or `None` when the skill does not exist.
    pub async fn fetch_latest_skill(&self, name: &SkillName) -> Result<Option<SkillVersion>> {
        let selector = Selector::Label("latest".to_owned());
        match self.get(name, &selector).await? {
            Ok(skill) => parse_skill_version(skill, name, Some(&selector)).map(Some),
            Err(rejected) if is_resource_not_found(&rejected.error) => Ok(None),
            Err(rejected) => Err(self.rejected(rejected)),
        }
    }

    /// Publish a new version from `files` (path, content).
    pub async fn create_skill_version<'a>(
        &self,
        name: &SkillName,
        files: impl Iterator<Item = (&'a str, &'a str)>,
        commit_message: Option<&str>,
    ) -> Result<SkillVersion> {
        let client = self.client()?;
        let request = CreateSkillVersionRequest {
            files: files
                .map(|(path, content)| SkillVersionFileCreateInput::content(path, content))
                .collect(),
            commit_message: commit_message.map(str::to_owned),
        };
        let skill = client
            .skills
            .create_version(&request)
            .await
            .map_err(|error| self.failure("POST", "/api/public/skills", error, INVALID_MANIFEST))?;
        parse_skill_version(skill, name, None)
    }

    /// Replace the labels of a version; exit 7 unless the response has them.
    pub async fn set_skill_labels(
        &self,
        name: &SkillName,
        version: Version,
        labels: &[String],
    ) -> Result<SkillVersion> {
        let client = self.client()?;
        let request = SetSkillVersionLabelsRequest {
            labels: labels.to_vec(),
        };
        let skill = client
            .skills
            .set_version_labels(name.as_str(), version.get(), &request)
            .await
            .map_err(|error| {
                let path = format!(
                    "/api/public/skills/{}/versions/{version}",
                    crate::operation::encode_segment(name.as_str())
                );
                self.failure("PATCH", &path, error, INVALID_MANIFEST)
            })?;
        let skill = parse_skill_version(skill, name, None)?;
        if skill.version != version || labels.iter().any(|label| !skill.labels.contains(label)) {
            return Err(invalid(
                "Langfuse returned a skill version that does not match the requested labels",
            ));
        }
        Ok(skill)
    }

    /// The contents of every hash, in batches fetched concurrently.
    pub async fn fetch_all_contents(&self, hashes: &[String]) -> Result<HashMap<String, String>> {
        let client = self.client()?;
        let maps: Vec<HashMap<String, String>> =
            stream::iter(hashes.chunks(FILE_CONTENT_BATCH_SIZE))
                .map(|batch| async move {
                    let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
                    let response =
                        client
                            .skills
                            .get_file_contents(&refs)
                            .await
                            .map_err(|error| {
                                self.failure(
                                    "GET",
                                    "/api/public/skills/files/content",
                                    error,
                                    INVALID_CONTENTS,
                                )
                            })?;
                    check_contents(
                        batch,
                        response
                            .data
                            .into_iter()
                            .map(|entry| (entry.sha256_hash, entry.content)),
                    )
                })
                .buffered(REQUEST_CONCURRENCY)
                .try_collect()
                .await?;
        Ok(maps.into_iter().flatten().collect())
    }
}

fn check_contents(
    requested: &[String],
    entries: impl Iterator<Item = (String, String)>,
) -> Result<HashMap<String, String>> {
    let wanted: HashSet<&str> = requested.iter().map(String::as_str).collect();
    let mut contents = HashMap::new();
    for (hash, content) in entries {
        if !is_sha256(&hash) || !wanted.contains(hash.as_str()) || contents.contains_key(&hash) {
            return Err(invalid(INVALID_CONTENTS));
        }
        contents.insert(hash, content);
    }
    if contents.len() != wanted.len() {
        return Err(invalid("Langfuse returned incomplete skill file contents"));
    }
    Ok(contents)
}

/// A 404 whose body says the skill (or label) does not exist.
fn is_resource_not_found(error: &LangfuseError) -> bool {
    if error.status() != Some(404) {
        return false;
    }
    let Some(body) = error.body().and_then(Value::as_object) else {
        return false;
    };
    body.get("message").is_some_and(Value::is_string)
        && (body.get("code").and_then(Value::as_str) == Some("resource_not_found")
            || body.get("error").and_then(Value::as_str) == Some("LangfuseNotFoundError"))
}

/// Validate a manifest: the name that was asked for, a positive version, at
/// least one file, canonical hashes, unique paths, and the requested selector.
fn parse_skill_version(
    skill: SdkSkillVersion,
    name: &SkillName,
    selector: Option<&Selector>,
) -> Result<SkillVersion> {
    let version = Version::new(skill.version).ok_or_else(|| invalid(INVALID_MANIFEST))?;
    if skill.name != name.as_str() || skill.files.is_empty() {
        return Err(invalid(INVALID_MANIFEST));
    }
    let mut paths = HashSet::new();
    let mut files = Vec::with_capacity(skill.files.len());
    for file in skill.files {
        let content_length =
            u64::try_from(file.content_length).map_err(|_| invalid(INVALID_MANIFEST))?;
        if !is_sha256(&file.sha256_hash) || !paths.insert(file.path.clone()) {
            return Err(invalid(INVALID_MANIFEST));
        }
        files.push(SkillFile {
            path: file.path,
            sha256_hash: file.sha256_hash,
            content_length,
        });
    }
    match selector {
        Some(Selector::Version(requested)) if version != *requested => {
            return Err(invalid(&format!(
                "Requested {name}@{requested}; server returned version {version}"
            )));
        }
        Some(Selector::Label(label)) if !skill.labels.contains(label) => {
            return Err(invalid(&format!(
                "Requested {name}@{label}; server returned labels {}",
                Value::from(skill.labels.as_slice())
            )));
        }
        _ => {}
    }
    Ok(SkillVersion {
        name: name.clone(),
        version,
        labels: skill.labels,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes() {
        let hash = sha256_base64(b"hello");
        assert!(is_sha256(&hash));
        assert!(!is_sha256("aGVsbG8="));
        assert!(!is_sha256(hash.trim_end_matches('=')));
    }
}
