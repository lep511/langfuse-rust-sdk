use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_yml::Value;

#[derive(Debug, thiserror::Error)]
pub enum YamlError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML parse error: {0}")]
    Parse(#[from] serde_yml::Error),
    #[error("Field not found: {0}")]
    FieldNotFound(String),
}

pub struct YamlTool;

impl YamlTool {
    // ── Read ────────────────────────────────────────────────────────

    /// Deserialize a YAML file into a typed struct.
    ///
    /// ```ignore
    /// #[derive(Deserialize)]
    /// struct Config { name: String, port: u16 }
    /// let cfg: Config = YamlTool::read_file("config.yaml")?;
    /// ```
    pub fn read_file<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, YamlError> {
        let contents = fs::read_to_string(path)?;
        Ok(serde_yml::from_str(&contents)?)
    }

    /// Deserialize a YAML string into a typed struct.
    pub fn from_str<T: DeserializeOwned>(yaml: &str) -> Result<T, YamlError> {
        Ok(serde_yml::from_str(yaml)?)
    }

    /// Read a YAML file into a dynamic `Value` (arbitrary structure).
    ///
    /// Useful when the schema is unknown or you need to inspect fields
    /// before deciding how to handle them.
    pub fn read_value(path: impl AsRef<Path>) -> Result<Value, YamlError> {
        let contents = fs::read_to_string(path)?;
        Ok(serde_yml::from_str(&contents)?)
    }

    /// Parse a YAML string into a dynamic `Value`.
    pub fn parse_value(yaml: &str) -> Result<Value, YamlError> {
        Ok(serde_yml::from_str(yaml)?)
    }

    // ── Write ───────────────────────────────────────────────────────

    /// Serialize a value to a YAML string.
    pub fn to_string<T: Serialize>(data: &T) -> Result<String, YamlError> {
        Ok(serde_yml::to_string(data)?)
    }

    /// Serialize a value and write it to a YAML file.
    pub fn write_file<T: Serialize>(
        path: impl AsRef<Path>,
        data: &T,
    ) -> Result<(), YamlError> {
        let yaml = serde_yml::to_string(data)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    // ── Query ───────────────────────────────────────────────────────

    /// Get a nested field from a `Value` using a dot-separated path.
    ///
    /// ```ignore
    /// let val = YamlTool::parse_value("server:\n  host: localhost")?;
    /// let host = YamlTool::get_field(&val, "server.host")?;
    /// assert_eq!(host.as_str(), Some("localhost"));
    /// ```
    pub fn get_field<'a>(value: &'a Value, path: &str) -> Result<&'a Value, YamlError> {
        let mut current = value;
        for key in path.split('.') {
            current = current
                .get(key)
                .ok_or_else(|| YamlError::FieldNotFound(path.to_string()))?;
        }
        Ok(current)
    }

    /// List the top-level keys of a YAML mapping.
    /// Returns an empty vec if the value is not a mapping.
    pub fn keys(value: &Value) -> Vec<String> {
        match value.as_mapping() {
            Some(map) => map
                .keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect(),
            None => Vec::new(),
        }
    }

    // ── Merge ───────────────────────────────────────────────────────

    /// Deep-merge `overlay` into `base`. Mappings are merged recursively;
    /// all other types in `overlay` replace the corresponding value in `base`.
    pub fn merge(base: &mut Value, overlay: &Value) {
        match (base, overlay) {
            (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
                for (key, overlay_val) in overlay_map {
                    match base_map.get_mut(key) {
                        Some(base_val) => Self::merge(base_val, overlay_val),
                        None => {
                            base_map.insert(key.clone(), overlay_val.clone());
                        }
                    }
                }
            }
            (base, overlay) => {
                *base = overlay.clone();
            }
        }
    }

    // ── Validate ────────────────────────────────────────────────────

    /// Check whether a string is valid YAML. Returns `Ok(())` on success
    /// or the parse error on failure.
    pub fn validate(yaml: &str) -> Result<(), YamlError> {
        let _: Value = serde_yml::from_str(yaml)?;
        Ok(())
    }

    // ── Convert ─────────────────────────────────────────────────────

    /// Convert a `serde_json::Value` to a YAML `Value`.
    pub fn from_json(json: &serde_json::Value) -> Result<Value, YamlError> {
        let yaml_str = serde_yml::to_string(json)?;
        Ok(serde_yml::from_str(&yaml_str)?)
    }

    /// Convert a YAML `Value` to a `serde_json::Value`.
    pub fn to_json(yaml: &Value) -> Result<serde_json::Value, YamlError> {
        let yaml_str = serde_yml::to_string(yaml)?;
        let json: serde_json::Value = serde_yml::from_str(&yaml_str)?;
        Ok(json)
    }

    // ── Frontmatter ─────────────────────────────────────────────────

    /// Extract YAML frontmatter delimited by `---` from a document.
    /// Returns `(frontmatter_value, remaining_body)`.
    ///
    /// Useful for parsing Langfuse SKILL.md files that start with
    /// YAML frontmatter.
    pub fn extract_frontmatter(text: &str) -> Result<(Value, &str), YamlError> {
        let trimmed = text.trim_start();
        if !trimmed.starts_with("---") {
            return Err(YamlError::FieldNotFound(
                "No YAML frontmatter found (must start with ---)".into(),
            ));
        }

        let after_first = &trimmed[3..];
        let end = after_first
            .find("\n---")
            .ok_or_else(|| YamlError::FieldNotFound("No closing --- for frontmatter".into()))?;

        let yaml_block = &after_first[..end];
        let body_start = end + 4; // skip "\n---"
        let body = after_first[body_start..].trim_start_matches('\n');

        let value: Value = serde_yml::from_str(yaml_block)?;
        Ok((value, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_yaml() {
        let yaml = "name: Alice\nage: 30\n";
        let val = YamlTool::parse_value(yaml).unwrap();
        assert_eq!(val["name"].as_str().unwrap(), "Alice");
        assert_eq!(val["age"].as_u64().unwrap(), 30);
    }

    #[test]
    fn parse_typed_yaml() {
        #[derive(serde::Deserialize, Debug, PartialEq)]
        struct Person {
            name: String,
            age: u32,
            active: bool,
        }

        let yaml = "name: Bob\nage: 25\nactive: true\n";
        let p: Person = YamlTool::from_str(yaml).unwrap();
        assert_eq!(p.name, "Bob");
        assert_eq!(p.age, 25);
        assert!(p.active);
    }

    #[test]
    fn roundtrip_serialize_deserialize() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Config {
            host: String,
            port: u16,
        }

        let original = Config {
            host: "localhost".into(),
            port: 8080,
        };
        let yaml = YamlTool::to_string(&original).unwrap();
        let restored: Config = YamlTool::from_str(&yaml).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn get_nested_field() {
        let yaml = "server:\n  host: localhost\n  port: 3000\n";
        let val = YamlTool::parse_value(yaml).unwrap();

        let host = YamlTool::get_field(&val, "server.host").unwrap();
        assert_eq!(host.as_str().unwrap(), "localhost");

        let port = YamlTool::get_field(&val, "server.port").unwrap();
        assert_eq!(port.as_u64().unwrap(), 3000);
    }

    #[test]
    fn get_field_not_found() {
        let val = YamlTool::parse_value("a: 1\n").unwrap();
        let err = YamlTool::get_field(&val, "b.c").unwrap_err();
        assert!(matches!(err, YamlError::FieldNotFound(_)));
    }

    #[test]
    fn list_keys() {
        let val = YamlTool::parse_value("name: x\nage: 1\nactive: true\n").unwrap();
        let keys = YamlTool::keys(&val);
        assert_eq!(keys, vec!["name", "age", "active"]);
    }

    #[test]
    fn keys_on_non_mapping() {
        let val = YamlTool::parse_value("- one\n- two\n").unwrap();
        assert!(YamlTool::keys(&val).is_empty());
    }

    #[test]
    fn deep_merge() {
        let mut base = YamlTool::parse_value(
            "server:\n  host: localhost\n  port: 3000\ndb: postgres\n",
        )
        .unwrap();
        let overlay = YamlTool::parse_value(
            "server:\n  port: 8080\n  tls: true\nredis: enabled\n",
        )
        .unwrap();

        YamlTool::merge(&mut base, &overlay);

        assert_eq!(
            YamlTool::get_field(&base, "server.host")
                .unwrap()
                .as_str()
                .unwrap(),
            "localhost"
        );
        assert_eq!(
            YamlTool::get_field(&base, "server.port")
                .unwrap()
                .as_u64()
                .unwrap(),
            8080
        );
        assert_eq!(
            YamlTool::get_field(&base, "server.tls")
                .unwrap()
                .as_bool()
                .unwrap(),
            true
        );
        assert_eq!(base["db"].as_str().unwrap(), "postgres");
        assert_eq!(base["redis"].as_str().unwrap(), "enabled");
    }

    #[test]
    fn validate_valid_yaml() {
        assert!(YamlTool::validate("key: value\n").is_ok());
    }

    #[test]
    fn validate_invalid_yaml() {
        assert!(YamlTool::validate(":\n  - :\n  invalid: [").is_err());
    }

    #[test]
    fn yaml_sequences() {
        let yaml = "fruits:\n  - apple\n  - banana\n  - cherry\n";
        let val = YamlTool::parse_value(yaml).unwrap();
        let fruits = val["fruits"].as_sequence().unwrap();
        assert_eq!(fruits.len(), 3);
        assert_eq!(fruits[0].as_str().unwrap(), "apple");
        assert_eq!(fruits[2].as_str().unwrap(), "cherry");
    }

    #[test]
    fn yaml_to_json_conversion() {
        let yaml = YamlTool::parse_value("name: test\ncount: 42\n").unwrap();
        let json = YamlTool::to_json(&yaml).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["count"], 42);
    }

    #[test]
    fn json_to_yaml_conversion() {
        let json = serde_json::json!({"name": "test", "count": 42});
        let yaml = YamlTool::from_json(&json).unwrap();
        assert_eq!(yaml["name"].as_str().unwrap(), "test");
        assert_eq!(yaml["count"].as_u64().unwrap(), 42);
    }

    #[test]
    fn extract_frontmatter_success() {
        let doc = "---\nname: my-skill\ndescription: A skill\n---\n# Title\n\nBody text here.\n";
        let (fm, body) = YamlTool::extract_frontmatter(doc).unwrap();
        assert_eq!(fm["name"].as_str().unwrap(), "my-skill");
        assert_eq!(fm["description"].as_str().unwrap(), "A skill");
        assert!(body.starts_with("# Title"));
    }

    #[test]
    fn extract_frontmatter_missing() {
        let doc = "# No frontmatter\nJust text.\n";
        assert!(YamlTool::extract_frontmatter(doc).is_err());
    }

    #[test]
    fn file_roundtrip() {
        #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
        struct Settings {
            debug: bool,
            level: String,
        }

        let data = Settings {
            debug: true,
            level: "info".into(),
        };

        let path = std::env::temp_dir().join("yaml_tool_test.yaml");
        YamlTool::write_file(&path, &data).unwrap();
        let loaded: Settings = YamlTool::read_file(&path).unwrap();
        assert_eq!(data, loaded);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn default_values_with_option() {
        #[derive(serde::Deserialize, Debug)]
        struct Config {
            name: String,
            #[serde(default)]
            port: u16,
            #[serde(default)]
            debug: bool,
        }

        let yaml = "name: myapp\n";
        let cfg: Config = YamlTool::from_str(yaml).unwrap();
        assert_eq!(cfg.name, "myapp");
        assert_eq!(cfg.port, 0);
        assert!(!cfg.debug);
    }

    #[test]
    fn deny_unknown_fields() {
        #[derive(serde::Deserialize, Debug)]
        #[serde(deny_unknown_fields)]
        struct Strict {
            #[allow(dead_code)]
            name: String,
        }

        let yaml = "name: ok\nextra: bad\n";
        let result: Result<Strict, _> = YamlTool::from_str(yaml);
        assert!(result.is_err());
    }
}
