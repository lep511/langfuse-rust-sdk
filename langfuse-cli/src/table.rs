//! `--format table`: list items as rows, or an object's fields as key/value
//! rows, sized to the terminal width.

use comfy_table::presets::UTF8_FULL_CONDENSED;
use comfy_table::{ContentArrangement, Table};
use serde_json::Value;

use crate::output::{escape_terminal_text, pretty};

/// Keys preferred, in this order, when a list has no default columns.
const PREFERRED: &[&str] = &[
    "id",
    "name",
    "type",
    "version",
    "status",
    "value",
    "createdAt",
    "updatedAt",
    "timestamp",
];
/// Columns picked automatically.
const AUTO_COLUMNS: usize = 6;
/// Longest cell text before it is cut with `…`.
const MAX_CELL: usize = 120;

/// The table for `body`, with `columns` (dotted paths such as `usage.total`)
/// or automatic ones.
pub fn render(body: &Value, columns: &[String], width: Option<u16>) -> String {
    let mut table = Table::new();
    table
        .load_style(UTF8_FULL_CONDENSED)
        .set_content_arrangement(ContentArrangement::Dynamic);
    if let Some(width) = width {
        table.set_width(width);
    }

    let items = body
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    match (items, body) {
        (Some(items), _) => {
            let columns = if columns.is_empty() {
                auto_columns(items)
            } else {
                columns.to_vec()
            };
            table.set_header(columns.iter().map(|column| escape_terminal_text(column)));
            for item in items {
                table.add_row(columns.iter().map(|column| cell(lookup(item, column))));
            }
            let mut rendered = table.to_string();
            if let Some(meta) = body.get("meta").filter(|meta| meta.is_object()) {
                rendered.push('\n');
                rendered.push_str(&escape_terminal_text(&meta.to_string()));
            }
            rendered
        }
        (None, Value::Object(fields)) => {
            table.set_header(["field", "value"]);
            let selected: Vec<(String, Option<&Value>)> = if columns.is_empty() {
                fields
                    .iter()
                    .map(|(key, value)| (key.clone(), Some(value)))
                    .collect()
            } else {
                columns
                    .iter()
                    .map(|column| (column.clone(), lookup(body, column)))
                    .collect()
            };
            for (key, value) in selected {
                table.add_row([escape_terminal_text(&key), cell(value)]);
            }
            table.to_string()
        }
        (None, other) => pretty(other),
    }
}

/// The value at a dotted path (`usage.total`, `labels.0`).
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(value, |current, key| match current {
            Value::Object(fields) => fields.get(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|index| items.get(index)),
            _ => None,
        })
}

fn cell(value: Option<&Value>) -> String {
    let text = match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) if items.iter().all(is_scalar) => items
            .iter()
            .map(|item| match item {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
    };
    let text = escape_terminal_text(&text);
    if text.chars().count() > MAX_CELL {
        let cut: String = text.chars().take(MAX_CELL - 1).collect();
        format!("{cut}…")
    } else {
        text
    }
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

/// Preferred keys first, then other scalar keys of the first item.
fn auto_columns(items: &[Value]) -> Vec<String> {
    let Some(Value::Object(first)) = items.first() else {
        return vec!["value".to_owned()];
    };
    let mut columns: Vec<String> = PREFERRED
        .iter()
        .filter(|key| first.contains_key(**key))
        .map(|key| (*key).to_owned())
        .collect();
    for (key, value) in first {
        if columns.len() >= AUTO_COLUMNS {
            break;
        }
        if !columns.contains(key)
            && (is_scalar(value)
                || value
                    .as_array()
                    .is_some_and(|items| items.iter().all(is_scalar)))
        {
            columns.push(key.clone());
        }
    }
    columns.truncate(AUTO_COLUMNS);
    columns
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn list_rows_with_chosen_columns_and_meta() {
        let body = json!({
            "data": [
                {"name": "a", "labels": ["production", "latest"], "usage": {"total": 3}},
                {"name": "b\x1b[31m", "labels": [], "usage": {"total": 4}}
            ],
            "meta": {"page": 1, "totalPages": 1}
        });
        let rendered = render(
            &body,
            &["name".into(), "labels".into(), "usage.total".into()],
            Some(80),
        );
        assert!(rendered.contains("production, latest"), "{rendered}");
        assert!(rendered.contains("usage.total"), "{rendered}");
        assert!(rendered.contains("b\\u001b[31m"), "{rendered}");
        assert!(
            rendered.ends_with(r#"{"page":1,"totalPages":1}"#),
            "{rendered}"
        );
    }

    #[test]
    fn automatic_columns_prefer_id_and_name() {
        let items = [json!({"other": 1, "name": "x", "id": "1", "nested": {"a": 1}})];
        assert_eq!(auto_columns(&items), ["id", "name", "other"]);
    }

    #[test]
    fn object_as_fields() {
        let rendered = render(&json!({"status": "OK", "version": "4.78.0"}), &[], Some(60));
        assert!(
            rendered.contains("status") && rendered.contains("4.78.0"),
            "{rendered}"
        );
    }
}
