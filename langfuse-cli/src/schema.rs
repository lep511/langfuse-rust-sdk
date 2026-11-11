//! `langfuse api schema --json`: every command with its operation, built from
//! the clap command tree and the operations' [`OperationMeta`].

use std::collections::BTreeMap;

use clap::builder::ValueParser;
use clap::{ArgAction, Subcommand};
use serde_json::{Map, Value, json};

use crate::api::{Resource, SPEC};
use crate::execute::SDK_VERSION;
use crate::operation::{BodyKind, OperationMeta};

/// One `resource action` command and the operation it calls.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Canonical resource.
    pub resource: String,
    /// Canonical action.
    pub action: String,
    /// Other `resource action` spellings of the command.
    pub aliases: Vec<(String, String)>,
    /// The operation.
    pub meta: &'static OperationMeta,
    /// The clap command of the canonical spelling.
    pub command: clap::Command,
}

/// Every operation, keyed by operationId, with its canonical command and its
/// aliases (resource aliases, hidden alias resources, actions shared by two
/// resources).
pub fn registry() -> BTreeMap<&'static str, Entry> {
    let root = Resource::augment_subcommands(clap::Command::new("api"));
    let mut entries: BTreeMap<&'static str, Entry> = BTreeMap::new();
    let mut aliases: BTreeMap<&'static str, Vec<(String, String)>> = BTreeMap::new();
    for (resource, metas) in root.get_subcommands().zip(Resource::metas()) {
        let resource_name = resource.get_name().to_owned();
        let resource_aliases: Vec<String> = resource.get_all_aliases().map(str::to_owned).collect();
        for (action, meta) in resource.get_subcommands().zip(metas) {
            let action_name = action.get_name().to_owned();
            let spelled = aliases.entry(meta.operation_id).or_default();
            for alias in &resource_aliases {
                spelled.push((alias.clone(), action_name.clone()));
            }
            if meta.resource == resource_name && meta.action == action_name {
                entries.insert(
                    meta.operation_id,
                    Entry {
                        resource: resource_name.clone(),
                        action: action_name,
                        aliases: Vec::new(),
                        meta,
                        command: action.clone(),
                    },
                );
            } else {
                spelled.push((resource_name.clone(), action_name));
            }
        }
    }
    for (operation_id, mut spelled) in aliases {
        if let Some(entry) = entries.get_mut(operation_id) {
            spelled.sort();
            spelled.dedup();
            entry.aliases = spelled;
        }
    }
    entries
}

/// `from-updated-at` → `fromUpdatedAt`.
fn camel_case(kebab: &str) -> String {
    let mut out = String::with_capacity(kebab.len());
    let mut upper = false;
    for character in kebab.chars() {
        if character == '-' || character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

/// Field flags whose names differ from the wire field.
const WIRE_OVERRIDES: &[(&str, &str)] = &[
    ("provider-secret-key", "secretKey"),
    ("predefined-public-key", "publicKey"),
    ("predefined-secret-key", "secretKey"),
    ("base-url", "baseURL"),
];

fn kind(parser: &ValueParser, action: &ArgAction) -> &'static str {
    if matches!(action, ArgAction::SetTrue | ArgAction::SetFalse) {
        return "boolean";
    }
    let id = parser.type_id();
    let is = |reference: ValueParser| reference.type_id() == id;
    if is(clap::value_parser!(i64).into())
        || is(clap::value_parser!(u64).into())
        || is(clap::value_parser!(u32).into())
    {
        "integer"
    } else if is(clap::value_parser!(f64).into()) {
        "number"
    } else if is(ValueParser::bool()) {
        "boolean"
    } else if is(ValueParser::new(crate::args::json_value)) {
        "json"
    } else {
        "string"
    }
}

fn parameters(entry: &Entry) -> Vec<Value> {
    let path_names = entry.meta.path_parameters();
    let mut positional = 0;
    let mut out = Vec::new();
    for arg in entry.command.get_arguments() {
        if arg.is_hide_set() {
            continue;
        }
        let heading = arg.get_help_heading().unwrap_or_default();
        let (location, cli_name, wire) = match arg.get_long() {
            None => {
                let wire = path_names
                    .get(positional)
                    .copied()
                    .unwrap_or_default()
                    .to_owned();
                positional += 1;
                (
                    "path",
                    arg.get_value_names()
                        .and_then(|names| names.first())
                        .map(ToString::to_string)
                        .unwrap_or_else(|| arg.get_id().to_string()),
                    wire,
                )
            }
            Some(long) => {
                let location = match (heading, long) {
                    (_, "body-json" | "body-file") => "body-channel",
                    (_, "all" | "max-items") => "cli",
                    ("Request body", _) => "body",
                    _ => "query",
                };
                let wire = WIRE_OVERRIDES
                    .iter()
                    .find(|(flag, _)| *flag == long && location == "body")
                    .map(|(_, wire)| (*wire).to_owned())
                    .unwrap_or_else(|| camel_case(long));
                (location, long.to_owned(), wire)
            }
        };
        let mut parameter = Map::new();
        parameter.insert("location".into(), json!(location));
        if location == "path" || location == "query" || location == "body" {
            parameter.insert("name".into(), json!(wire));
        }
        parameter.insert("cliName".into(), json!(cli_name));
        let aliases: Vec<&str> = arg.get_all_aliases().unwrap_or_default();
        if !aliases.is_empty() {
            parameter.insert("cliAliases".into(), json!(aliases));
        }
        parameter.insert("required".into(), json!(arg.is_required_set()));
        parameter.insert(
            "kind".into(),
            json!(kind(arg.get_value_parser(), arg.get_action())),
        );
        if matches!(arg.get_action(), ArgAction::Append) {
            parameter.insert("repeatable".into(), json!(true));
        }
        let values: Vec<String> = arg
            .get_possible_values()
            .iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_owned())
            .collect();
        if !values.is_empty() && !matches!(arg.get_action(), ArgAction::SetTrue) {
            parameter.insert("enum".into(), json!(values));
        }
        if let Some(help) = arg.get_help() {
            parameter.insert("description".into(), json!(help.to_string()));
        }
        out.push(Value::Object(parameter));
    }
    out
}

fn action(entry: &Entry) -> Value {
    let meta = entry.meta;
    let mut action = Map::new();
    action.insert("name".into(), json!(entry.action));
    action.insert(
        "aliases".into(),
        Value::Array(
            entry
                .aliases
                .iter()
                .map(|(resource, action)| json!({"resource": resource, "action": action}))
                .collect(),
        ),
    );
    action.insert("operationId".into(), json!(meta.operation_id));
    action.insert("method".into(), json!(meta.method.as_str()));
    action.insert("path".into(), json!(meta.path));
    action.insert("deprecated".into(), json!(false));
    if let Some(pagination) = meta.pagination {
        action.insert("pagination".into(), json!(pagination.as_str()));
    }
    action.insert(
        "auth".into(),
        if meta.auth {
            json!({"required": true, "schemes": ["BasicAuth"]})
        } else {
            json!({"required": false, "schemes": []})
        },
    );
    action.insert("pathParameterOrder".into(), json!(meta.path_parameters()));
    action.insert("parameters".into(), Value::Array(parameters(entry)));
    match meta.body {
        BodyKind::None => {}
        BodyKind::Fields => {
            action.insert(
                "requestBody".into(),
                json!({"required": true, "contentType": "application/json", "fieldFlags": true}),
            );
        }
        BodyKind::Union { discriminator } => {
            action.insert(
                "requestBody".into(),
                json!({"required": true, "contentType": "application/json", "fieldFlags": true,
                       "union": true, "discriminator": {"cliName": discriminator}}),
            );
        }
        BodyKind::JsonOnly => {
            action.insert(
                "requestBody".into(),
                json!({"required": true, "contentType": "application/json", "fieldFlags": false}),
            );
        }
    }
    if let Some(about) = entry.command.get_about() {
        action.insert("summary".into(), json!(about.to_string()));
    }
    if let Some(long) = entry.command.get_long_about() {
        action.insert("description".into(), json!(long.to_string()));
    }
    Value::Object(action)
}

/// The schema document (compact JSON on one line when printed).
pub fn schema() -> Value {
    let mut resources: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for entry in registry().into_values() {
        resources
            .entry(entry.resource.clone())
            .or_default()
            .push(entry);
    }
    let resource_aliases: BTreeMap<String, Vec<String>> =
        Resource::augment_subcommands(clap::Command::new("api"))
            .get_subcommands()
            .map(|resource| {
                (
                    resource.get_name().to_owned(),
                    resource.get_all_aliases().map(str::to_owned).collect(),
                )
            })
            .collect();
    json!({
        "schemaVersion": 1,
        "cli": {"name": "langfuse-rust-cli", "version": env!("CARGO_PKG_VERSION")},
        "sdk": {"name": "langfuse-rust-sdk", "version": SDK_VERSION},
        "apiVersion": SPEC,
        "resources": resources.into_iter().map(|(name, mut entries)| {
            entries.sort_by(|a, b| a.action.cmp(&b.action));
            json!({
                "name": name,
                "aliases": resource_aliases.get(&name).cloned().unwrap_or_default(),
                "actions": entries.iter().map(action).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_of_flags() {
        assert_eq!(camel_case("from-updated-at"), "fromUpdatedAt");
        assert_eq!(camel_case("sha256-hashes"), "sha256Hashes");
        assert_eq!(camel_case("name"), "name");
    }
}
