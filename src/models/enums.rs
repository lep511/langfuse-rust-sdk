//! Shared enums used across resources.
//!
//! Every enum that is deserialized from API responses is `#[non_exhaustive]` and has an
//! `Unknown` variant (`#[serde(other)]`), so a value added server-side does not break
//! deserialization of the whole response. `Unknown` cannot be serialized.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ModelUsageUnit {
    Characters,
    Tokens,
    Milliseconds,
    Seconds,
    Images,
    Requests,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ObservationLevel {
    Debug,
    Default,
    Warning,
    Error,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum DatasetStatus {
    Active,
    Archived,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ScoreSource {
    Annotation,
    Api,
    Eval,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ScoreDataType {
    Numeric,
    Boolean,
    Categorical,
    Correction,
    Text,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum ScoreConfigDataType {
    Numeric,
    Boolean,
    Categorical,
    Text,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PromptType {
    Chat,
    Text,
    /// A value not known to this SDK version.
    #[serde(other, skip_serializing)]
    Unknown,
}
