//! Every `api` operation, prepared with placeholder arguments: the request
//! has the operation's method and its path, with each path parameter filled
//! in path order.

mod common;

use langfuse_rust_cli::Exit;
use serde_json::Value;

/// A value clap accepts for a required query flag; `text` is tried for
/// strings, which may have to be JSON or a date-time.
fn placeholder(parameter: &Value, text: &str) -> String {
    if let Some(first) = parameter["enum"]
        .as_array()
        .and_then(|values| values.first())
    {
        return first.as_str().unwrap().to_owned();
    }
    match parameter["kind"].as_str().unwrap() {
        "integer" | "number" => "1".to_owned(),
        "boolean" => "true".to_owned(),
        "json" => "{}".to_owned(),
        _ => text.to_owned(),
    }
}

#[tokio::test]
async fn every_operation_fills_its_path_parameters() {
    let schema = common::run(&["--json", "api", "schema"]).await.json();
    let mut checked = 0;
    for resource in schema["resources"].as_array().unwrap() {
        for action in resource["actions"].as_array().unwrap() {
            let command = format!("{} {}", resource["name"], action["name"]);
            let parameters = action["parameters"].as_array().unwrap();
            let path_values: Vec<String> = parameters
                .iter()
                .filter(|parameter| parameter["location"] == "path")
                .enumerate()
                .map(|(index, parameter)| {
                    if parameter["kind"] == "string" {
                        format!("v{index}")
                    } else {
                        (index + 1).to_string()
                    }
                })
                .collect();
            let mut expected = action["path"].as_str().unwrap().to_owned();
            for (name, value) in action["pathParameterOrder"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&path_values)
            {
                expected = expected.replace(&format!("{{{}}}", name.as_str().unwrap()), value);
            }

            let mut curl = None;
            for text in ["x", "{}", "2026-11-01T00:00:00Z"] {
                let mut args = vec![
                    "api".to_owned(),
                    resource["name"].as_str().unwrap().to_owned(),
                    action["name"].as_str().unwrap().to_owned(),
                ];
                args.extend(path_values.iter().cloned());
                if action.get("requestBody").is_some() {
                    args.extend(["--body-json".to_owned(), "{}".to_owned()]);
                }
                for parameter in parameters {
                    if parameter["location"] == "query" && parameter["required"] == true {
                        args.push(format!("--{}", parameter["cliName"].as_str().unwrap()));
                        args.push(placeholder(parameter, text));
                    }
                }
                args.extend(
                    [
                        "--curl",
                        "--host",
                        "http://h",
                        "--public-key",
                        "pk",
                        "--secret-key",
                        "sk",
                    ]
                    .map(str::to_owned),
                );
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                let output = common::run(&args).await;
                if output.exit == Exit::Success {
                    curl = Some(output.stdout);
                    break;
                }
                assert_eq!(output.exit, Exit::Usage, "{command}: {output:?}");
            }
            let curl = curl.unwrap_or_else(|| panic!("{command}: no placeholder was accepted"));

            let words: Vec<&str> = curl.split(' ').collect();
            assert_eq!(words[..2], ["curl", "--request"], "{command}: {curl}");
            assert_eq!(words[2], action["method"].as_str().unwrap(), "{command}");
            let url = words[3].trim_matches('\'');
            let path = url
                .strip_prefix("http://h")
                .unwrap_or_else(|| panic!("{command}: {url}"));
            let path = path.split('?').next().unwrap();
            assert_eq!(path, expected, "{command}");
            assert!(
                !path.contains('{') && !path.contains("%7B"),
                "{command}: {path}"
            );
            checked += 1;
        }
    }
    let operations = if cfg!(feature = "unstable") { 120 } else { 99 };
    assert_eq!(checked, operations);
}
