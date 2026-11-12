//! Validated values of the skills API. Building one is the validation: code
//! that holds a [`SkillName`] or a [`Version`] does not check it again.

use std::fmt;

use serde::{Deserialize, Serialize, Serializer};

/// A skill name: lowercase letters, digits and single hyphens
/// (`^[a-z0-9]+(?:-[a-z0-9]+)*$`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct SkillName(String);

impl SkillName {
    /// `name` as a skill name, unless it is not one.
    pub fn new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();
        let valid = !name.is_empty()
            && !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--")
            && name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then_some(Self(name))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SkillName {
    type Error = &'static str;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::new(name).ok_or("expected a skill name (lowercase letters, digits, single hyphens)")
    }
}

impl fmt::Display for SkillName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for SkillName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// A skill version number: a positive integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "i64")]
pub struct Version(i64);

impl Version {
    /// `version` as a version number, unless it is zero or negative.
    pub fn new(version: i64) -> Option<Self> {
        (version > 0).then_some(Self(version))
    }

    /// The number, as the API takes it.
    pub fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for Version {
    type Error = &'static str;

    fn try_from(version: i64) -> Result<Self, Self::Error> {
        Self::new(version).ok_or("expected a positive version number")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_names() {
        for valid in ["a", "a1", "my-skill", "x-1-y"] {
            assert_eq!(SkillName::new(valid).unwrap().as_str(), valid);
        }
        for invalid in ["", "-a", "a-", "a--b", "A", "a_b", "a b", "ñ"] {
            assert_eq!(SkillName::new(invalid), None, "{invalid}");
        }
        assert_eq!(
            serde_json::from_str::<SkillName>("\"my-skill\"")
                .unwrap()
                .to_string(),
            "my-skill"
        );
        assert!(serde_json::from_str::<SkillName>("\"My Skill\"").is_err());
        assert_eq!(
            serde_json::to_string(&SkillName::new("a").unwrap()).unwrap(),
            "\"a\""
        );
    }

    #[test]
    fn versions() {
        assert_eq!(Version::new(3).unwrap().get(), 3);
        assert_eq!(Version::new(0), None);
        assert_eq!(Version::new(-1), None);
        assert!(Version::new(1) < Version::new(2));
        assert_eq!(
            serde_json::from_str::<Version>("7").unwrap().to_string(),
            "7"
        );
        for invalid in ["0", "-2", "1.5", "\"1\""] {
            assert!(
                serde_json::from_str::<Version>(invalid).is_err(),
                "{invalid}"
            );
        }
        assert_eq!(
            serde_json::to_string(&Version::new(7).unwrap()).unwrap(),
            "7"
        );
    }
}
