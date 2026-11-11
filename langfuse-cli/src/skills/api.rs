//! The Langfuse skills API as `skills` commands use it: the SDK's typed
//! `client.skills` methods, plus the official CLI's validation of every
//! response (exit 7 when a manifest does not match what was requested).

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
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

/// Hashes per file-content request.
pub const FILE_CONTENT_BATCH_SIZE: usize = 50;
/// Pages of the skill list read before giving up.
const MAX_SKILL_LIST_PAGES: i64 = 100;
/// File-content requests in flight at once.
const DOWNLOAD_CONCURRENCY: usize = 4;

/// `^[a-z0-9]+(?:-[a-z0-9]+)*$`.
pub fn is_skill_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

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
    pub name: String,
    /// Version number.
    pub version: i64,
    /// Labels of the version.
    pub labels: Vec<String>,
    /// Manifest.
    pub files: Vec<SkillFile>,
}

/// One entry of the skill list.
#[derive(Debug, Clone)]
pub struct SkillSummary {
    /// Skill name.
    pub name: String,
    /// The version labelled `production`, if any.
    pub production_version: Option<i64>,
}

/// The skills API of one connection.
pub struct Remote {
    settings: Settings,
    client: Option<LangfuseClient>,
}

fn invalid(message: &str) -> CliError {
    CliError::response(message)
}

impl Remote {
    /// A connection; credentials are checked at the first request.
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            client: None,
        }
    }

    fn client(&mut self) -> Result<LangfuseClient> {
        self.settings.require_credentials()?;
        if self.client.is_none() {
            self.client = Some(self.settings.client()?);
        }
        Ok(self.client.clone().expect("client was just built"))
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

    /// [`failure`](Self::failure) of a GET from [`get`](Self::get).
    fn failure_boxed(&self, failed: (String, LangfuseError), invalid_message: &str) -> CliError {
        let (path, error) = failed;
        self.failure("GET", &path, error, invalid_message)
    }

    /// Every skill (optionally with `tag`), deduplicated by name.
    pub async fn fetch_skills(&mut self, tag: Option<&str>) -> Result<Vec<SkillSummary>> {
        let client = self.client()?;
        let mut order: Vec<String> = Vec::new();
        let mut skills: HashMap<String, SkillSummary> = HashMap::new();
        for page in 1..=MAX_SKILL_LIST_PAGES {
            let params = ListSkillsParams {
                page: Some(page),
                limit: Some(100),
                tag: tag.map(str::to_owned),
                ..ListSkillsParams::default()
            };
            let path = format!(
                "/api/public/skills?{}",
                serde_urlencoded::to_string(&params).unwrap_or_default()
            );
            let response = client.skills.list(&params).await.map_err(|error| {
                self.failure(
                    "GET",
                    &path,
                    error,
                    "Langfuse returned an invalid skill list",
                )
            })?;
            for skill in response.data {
                if !is_skill_name(&skill.name)
                    || skill.production_version.is_some_and(|version| version <= 0)
                {
                    return Err(invalid("Langfuse returned an invalid skill list"));
                }
                if !skills.contains_key(&skill.name) {
                    order.push(skill.name.clone());
                }
                skills.insert(
                    skill.name.clone(),
                    SkillSummary {
                        name: skill.name,
                        production_version: skill.production_version,
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
        &mut self,
        name: &str,
        selector: &Selector,
    ) -> std::result::Result<SdkSkillVersion, Box<(String, LangfuseError)>> {
        let params = match selector {
            Selector::Label(label) => GetSkillParams {
                label: Some(label.clone()),
                ..GetSkillParams::default()
            },
            Selector::Version(version) => GetSkillParams {
                version: Some(*version),
                ..GetSkillParams::default()
            },
        };
        let path = format!(
            "/api/public/skills/{}?{}",
            crate::operation::encode_segment(name),
            serde_urlencoded::to_string(&params).unwrap_or_default()
        );
        let client = match self.client() {
            Ok(client) => client,
            Err(error) => return Err(Box::new((path, LangfuseError::Config(error.message)))),
        };
        client
            .skills
            .get(name, &params)
            .await
            .map_err(|error| Box::new((path, error)))
    }

    /// The version `selector` names; exit 7 unless it matches.
    pub async fn fetch_skill(&mut self, name: &str, selector: &Selector) -> Result<SkillVersion> {
        self.settings.require_credentials()?;
        match self.get(name, selector).await {
            Ok(skill) => parse_skill_version(skill, name, Some(selector)),
            Err(failed) => {
                Err(self.failure_boxed(*failed, "Langfuse returned an invalid skill manifest"))
            }
        }
    }

    /// The `latest` version, or `None` when the skill does not exist.
    pub async fn fetch_latest_skill(&mut self, name: &str) -> Result<Option<SkillVersion>> {
        self.settings.require_credentials()?;
        let selector = Selector::Label("latest".to_owned());
        match self.get(name, &selector).await {
            Ok(skill) => parse_skill_version(skill, name, Some(&selector)).map(Some),
            Err(failed) if is_resource_not_found(&failed.1) => Ok(None),
            Err(failed) => {
                Err(self.failure_boxed(*failed, "Langfuse returned an invalid skill manifest"))
            }
        }
    }

    /// Publish a new version from `files` (path, content).
    pub async fn create_skill_version(
        &mut self,
        name: &str,
        files: &[(String, String)],
        commit_message: Option<&str>,
    ) -> Result<SkillVersion> {
        let client = self.client()?;
        let request = CreateSkillVersionRequest {
            files: files
                .iter()
                .map(|(path, content)| {
                    SkillVersionFileCreateInput::content(path.clone(), content.clone())
                })
                .collect(),
            commit_message: commit_message.map(str::to_owned),
        };
        let skill = client
            .skills
            .create_version(&request)
            .await
            .map_err(|error| {
                self.failure(
                    "POST",
                    "/api/public/skills",
                    error,
                    "Langfuse returned an invalid skill manifest",
                )
            })?;
        parse_skill_version(skill, name, None)
    }

    /// Replace the labels of a version; exit 7 unless the response has them.
    pub async fn set_skill_labels(
        &mut self,
        name: &str,
        version: i64,
        labels: &[String],
    ) -> Result<SkillVersion> {
        let client = self.client()?;
        let path = format!(
            "/api/public/skills/{}/versions/{version}",
            crate::operation::encode_segment(name)
        );
        let skill = client
            .skills
            .set_version_labels(
                name,
                version,
                &SetSkillVersionLabelsRequest {
                    labels: labels.to_vec(),
                },
            )
            .await
            .map_err(|error| {
                self.failure(
                    "PATCH",
                    &path,
                    error,
                    "Langfuse returned an invalid skill manifest",
                )
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
    pub async fn fetch_all_contents(
        &mut self,
        hashes: &[String],
    ) -> Result<HashMap<String, String>> {
        use futures_util::stream::{self, StreamExt, TryStreamExt};
        let client = self.client()?;
        let settings = self.settings.clone();
        let batches: Vec<Vec<String>> = hashes
            .chunks(FILE_CONTENT_BATCH_SIZE)
            .map(<[String]>::to_vec)
            .collect();
        let maps: Vec<HashMap<String, String>> = stream::iter(batches)
            .map(|batch| {
                let client = client.clone();
                let settings = settings.clone();
                async move {
                    let refs: Vec<&str> = batch.iter().map(String::as_str).collect();
                    let response =
                        client
                            .skills
                            .get_file_contents(&refs)
                            .await
                            .map_err(|error| {
                                Remote::new(settings).failure(
                                    "GET",
                                    "/api/public/skills/files/content",
                                    error,
                                    "Langfuse returned invalid skill file contents",
                                )
                            })?;
                    check_contents(
                        &batch,
                        response
                            .data
                            .into_iter()
                            .map(|entry| (entry.sha256_hash, entry.content)),
                    )
                }
            })
            .buffered(DOWNLOAD_CONCURRENCY)
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
            return Err(invalid("Langfuse returned invalid skill file contents"));
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

/// Validate a manifest: a valid name equal to `name`, a positive version, at
/// least one file, canonical hashes, unique paths, and the requested selector.
fn parse_skill_version(
    skill: SdkSkillVersion,
    name: &str,
    selector: Option<&Selector>,
) -> Result<SkillVersion> {
    let manifest = "Langfuse returned an invalid skill manifest";
    if !is_skill_name(&skill.name)
        || skill.name != name
        || skill.version <= 0
        || skill.files.is_empty()
    {
        return Err(invalid(manifest));
    }
    let mut paths = HashSet::new();
    let mut files = Vec::with_capacity(skill.files.len());
    for file in skill.files {
        if !is_sha256(&file.sha256_hash)
            || file.content_length < 0
            || !paths.insert(file.path.clone())
        {
            return Err(invalid(manifest));
        }
        files.push(SkillFile {
            path: file.path,
            sha256_hash: file.sha256_hash,
            content_length: file.content_length as u64,
        });
    }
    match selector {
        Some(Selector::Version(version)) if skill.version != *version => {
            return Err(invalid(&format!(
                "Requested {name}@{version}; server returned version {}",
                skill.version
            )));
        }
        Some(Selector::Label(label)) if !skill.labels.contains(label) => {
            return Err(invalid(&format!(
                "Requested {name}@{label}; server returned labels {}",
                Value::from(skill.labels.clone())
            )));
        }
        _ => {}
    }
    Ok(SkillVersion {
        name: skill.name,
        version: skill.version,
        labels: skill.labels,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_hashes() {
        for valid in ["a", "a1", "my-skill", "x-1-y"] {
            assert!(is_skill_name(valid), "{valid}");
        }
        for invalid in ["", "-a", "a-", "a--b", "A", "a_b", "a b", "ñ"] {
            assert!(!is_skill_name(invalid), "{invalid}");
        }
        let hash = sha256_base64(b"hello");
        assert!(is_sha256(&hash));
        assert!(!is_sha256("aGVsbG8="));
        assert!(!is_sha256(hash.trim_end_matches('=')));
    }
}
