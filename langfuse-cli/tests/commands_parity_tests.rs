#![cfg(feature = "unstable")]
//! The `resource action` names of every operation match the official CLI's
//! (its 4.71.0 golden, plus the 4 Topics operations of the newer spec).

mod common;

use std::collections::BTreeMap;

use serde_json::Value;

type Command = (String, String, String, Vec<(String, String)>);

fn commands(entries: &[Value]) -> BTreeMap<String, Command> {
    entries
        .iter()
        .map(|entry| {
            let mut aliases: Vec<(String, String)> = entry["aliases"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|alias| {
                    (
                        alias["resource"].as_str().unwrap().to_owned(),
                        alias["action"].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
            aliases.sort();
            (
                entry["operationId"].as_str().unwrap().to_owned(),
                (
                    entry["resource"].as_str().unwrap().to_owned(),
                    entry["action"].as_str().unwrap().to_owned(),
                    entry["key"].as_str().unwrap().to_owned(),
                    aliases,
                ),
            )
        })
        .collect()
}

#[tokio::test]
async fn every_operation_has_the_official_command_name() {
    let fixture: Vec<Value> = serde_json::from_str(include_str!("fixtures/commands.json")).unwrap();
    let expected = commands(&fixture);
    assert_eq!(expected.len(), 120);

    let schema = common::run(&["--json", "api", "schema"]).await.json();
    let mut actual_entries = Vec::new();
    for resource in schema["resources"].as_array().unwrap() {
        for action in resource["actions"].as_array().unwrap() {
            actual_entries.push(serde_json::json!({
                "operationId": action["operationId"],
                "resource": resource["name"],
                "action": action["name"],
                "key": format!("{} {}", action["method"].as_str().unwrap(), action["path"].as_str().unwrap()),
                "aliases": action["aliases"],
            }));
        }
    }
    let actual = commands(&actual_entries);
    for (operation_id, command) in &expected {
        assert_eq!(actual.get(operation_id), Some(command), "{operation_id}");
    }
    assert_eq!(actual.len(), expected.len(), "unexpected extra operations");
}
