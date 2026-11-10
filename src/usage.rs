//! Token usage of LLM providers translated into Langfuse usage details.
//!
//! Langfuse computes costs from usage details keyed `input`, `output`,
//! `total` and their sub-types (`input_cache_read`, `input_cached_tokens`,
//! `output_reasoning`, ...). Providers report usage in their own shapes:
//! [`normalize_usage`] translates the common ones, as the Python SDK's
//! LangChain integration does (`_parse_usage_model`), so a generation traced
//! by hand from Anthropic, Amazon Bedrock, Google Vertex AI / Gemini, IBM or
//! LangChain-style usage gets the same usage details and costs as in Python.
//! [`Observation::set_provider_usage`](crate::otel::Observation::set_provider_usage)
//! (feature `otel`) applies it.
//!
//! ```
//! use langfuse_rust_sdk::usage::normalize_usage;
//! use serde_json::json;
//!
//! // Anthropic Messages API usage.
//! let usage = json!({
//!     "input_tokens": 9454,
//!     "output_tokens": 380,
//!     "cache_read_input_tokens": 1024,
//! });
//! assert_eq!(
//!     serde_json::Value::Object(normalize_usage(&usage).unwrap()),
//!     json!({"input": 9454, "output": 380, "cache_read_input_tokens": 1024}),
//! );
//! ```

use serde_json::{Map, Value};

/// Provider keys renamed to Langfuse keys, applied in this order (a later
/// key replaces the value an earlier one wrote).
const KEY_TRANSLATIONS: [(&str, &str); 13] = [
    // Anthropic (also on Bedrock), LangChain, OpenTelemetry GenAI.
    ("input_tokens", "input"),
    ("output_tokens", "output"),
    ("total_tokens", "total"),
    // OpenAI-style (e.g. the Bedrock InvokeModel API of some models).
    ("prompt_tokens", "input"),
    ("completion_tokens", "output"),
    // Google Vertex AI / Gemini.
    ("prompt_token_count", "input"),
    ("candidates_token_count", "output"),
    ("total_token_count", "total"),
    // Amazon Bedrock invocation metrics (lists when streamed).
    ("inputTokenCount", "input"),
    ("outputTokenCount", "output"),
    ("totalTokenCount", "total"),
    // IBM watsonx.
    ("input_token_count", "input"),
    ("generated_token_count", "output"),
];

/// Translate a provider's token usage into Langfuse usage details; `None`
/// when `usage` is not an object or holds no integer count.
///
/// A port of the Python SDK's `_parse_usage_model`:
///
/// - Plain OpenAI usage (`prompt_tokens`, `completion_tokens`,
///   `total_tokens`, optionally with exactly `prompt_tokens_details` and
///   `completion_tokens_details`) is returned unchanged: Langfuse maps it.
/// - The keys of Anthropic, OpenAI-style APIs, Vertex AI / Gemini, Bedrock
///   (summing the per-chunk lists of streams) and IBM become `input`,
///   `output` and `total`.
/// - `input_token_details` / `output_token_details` (LangChain) become
///   `input_<key>` / `output_<key>` and are subtracted from `input` /
///   `output` (except the `priority*` tiers, which are not sub-categories);
///   `prompt_tokens_details` (an object of OpenAI or LiteLLM cached tokens,
///   or a Vertex AI list per modality), `candidates_tokens_details` and
///   `cache_tokens_details` likewise, never below zero.
/// - Anthropic's `cache_creation` tiers become `cache_creation_<tier>`, plus
///   their sum as `cache_creation_input_tokens` unless the API sent it.
/// - Other integer keys are kept; everything that is not an integer is
///   dropped.
pub fn normalize_usage(usage: &Value) -> Option<Map<String, Value>> {
    let Value::Object(usage) = usage else {
        return None;
    };
    if is_plain_openai_usage(usage) {
        return Some(usage.clone());
    }
    let mut out = usage.clone();

    for (provider_key, langfuse_key) in KEY_TRANSLATIONS {
        if let Some(count) = out.remove(provider_key) {
            out.insert(langfuse_key.to_owned(), sum_if_list(count));
        }
    }

    if let Some(Value::Object(details)) = out.remove("input_token_details") {
        apply_token_details(&mut out, &details, "input");
    }
    if let Some(Value::Object(details)) = out.remove("output_token_details") {
        apply_token_details(&mut out, &details, "output");
    }

    match out.remove("prompt_tokens_details") {
        // OpenAI / LiteLLM, e.g. {"cached_tokens": 12000}.
        Some(Value::Object(details)) => {
            for (key, value) in details {
                if let Some(count) = int(&value) {
                    out.insert(format!("input_{key}"), count.into());
                    subtract(&mut out, "input", count);
                }
            }
        }
        // Vertex AI, e.g. [{"modality": "TEXT", "token_count": 800}].
        Some(Value::Array(details)) => {
            for (modality, count) in modality_counts(&details) {
                out.insert(format!("input_modality_{modality}"), count.into());
                subtract(&mut out, "input", count);
            }
        }
        Some(other) => {
            out.insert("prompt_tokens_details".into(), other);
        }
        None => {}
    }
    if let Some(Value::Array(details)) = out.get("candidates_tokens_details").cloned() {
        out.remove("candidates_tokens_details");
        for (modality, count) in modality_counts(&details) {
            out.insert(format!("output_modality_{modality}"), count.into());
            subtract(&mut out, "output", count);
        }
    }
    if let Some(Value::Array(details)) = out.get("cache_tokens_details").cloned() {
        out.remove("cache_tokens_details");
        for (modality, count) in modality_counts(&details) {
            out.insert(format!("cached_modality_{modality}"), count.into());
            subtract(&mut out, "input", count);
            subtract(&mut out, &format!("input_modality_{modality}"), count);
        }
    }

    // Anthropic extended prompt caching, e.g.
    // {"ephemeral_5m_input_tokens": 2048, "ephemeral_1h_input_tokens": 0}.
    if let Some(Value::Object(tiers)) = out.get("cache_creation").cloned() {
        out.remove("cache_creation");
        let mut total = 0;
        for (tier, value) in tiers {
            if let Some(count) = int(&value) {
                out.insert(format!("cache_creation_{tier}"), count.into());
                total += count;
            }
        }
        if total > 0 {
            out.entry("cache_creation_input_tokens")
                .or_insert_with(|| total.into());
        }
    }

    out.retain(|_, value| int(value).is_some());
    (!out.is_empty()).then_some(out)
}

/// Whether `usage` is exactly the OpenAI usage Langfuse maps on its own.
fn is_plain_openai_usage(usage: &Map<String, Value>) -> bool {
    let has = |keys: &[&str]| keys.iter().all(|key| usage.contains_key(*key));
    let basic = ["prompt_tokens", "completion_tokens", "total_tokens"];
    (usage.len() == 5
        && has(&basic)
        && has(&["prompt_tokens_details", "completion_tokens_details"]))
        || (usage.len() == 3 && has(&basic))
}

/// `input_token_details` / `output_token_details` of LangChain.
fn apply_token_details(out: &mut Map<String, Value>, details: &Map<String, Value>, side: &str) {
    for (key, value) in details {
        out.insert(format!("{side}_{key}"), value.clone());
        // Priority tiers are not exclusive sub-categories.
        if key == "priority" || key.starts_with("priority_") {
            continue;
        }
        if let Some(count) = int(value) {
            subtract(out, side, count);
        }
    }
}

/// `(modality, token_count)` of the Vertex AI per-modality lists.
fn modality_counts(details: &[Value]) -> Vec<(String, i64)> {
    details
        .iter()
        .filter_map(|item| {
            let modality = item.get("modality")?;
            let count = int(item.get("token_count")?)?;
            let modality = match modality {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Some((modality, count))
        })
        .collect()
}

/// `out[key] -= count`, never below zero, when `out[key]` is an integer.
fn subtract(out: &mut Map<String, Value>, key: &str, count: i64) {
    if let Some(current) = out.get(key).and_then(int) {
        out.insert(key.to_owned(), (current - count).max(0).into());
    }
}

/// The sum of a list of counts (Bedrock streams report one per chunk).
fn sum_if_list(value: Value) -> Value {
    match value {
        Value::Array(items) => {
            let counts: Option<Vec<i64>> = items.iter().map(int).collect();
            counts.map_or(Value::Array(items), |counts| {
                counts.iter().sum::<i64>().into()
            })
        }
        other => other,
    }
}

fn int(value: &Value) -> Option<i64> {
    value.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn normalized(usage: Value) -> Value {
        Value::Object(normalize_usage(&usage).expect("some usage"))
    }

    // The cases of the Python SDK's tests/unit/test_parse_usage_model.py.

    #[test]
    fn standard_tier_input_token_details() {
        let result = normalized(json!({
            "input_tokens": 13, "output_tokens": 1, "total_tokens": 14,
            "input_token_details": {"audio": 0, "cache_read": 3},
            "output_token_details": {"audio": 0},
        }));
        assert_eq!(
            result,
            json!({
                "input": 10, "output": 1, "total": 14,
                "input_audio": 0, "input_cache_read": 3, "output_audio": 0,
            })
        );
    }

    #[test]
    fn prompt_tokens_details_dict_cached_tokens() {
        let result = normalized(json!({
            "prompt_tokens": 15000, "completion_tokens": 500, "total_tokens": 15500,
            "prompt_tokens_details": {"cached_tokens": 12000},
        }));
        assert_eq!(
            result,
            json!({"input": 3000, "output": 500, "total": 15500, "input_cached_tokens": 12000})
        );
    }

    #[test]
    fn prompt_tokens_details_dict_with_cache_creation() {
        let result = normalized(json!({
            "prompt_tokens": 15000, "completion_tokens": 500, "total_tokens": 15500,
            "prompt_tokens_details": {"cached_tokens": 12000},
            "cache_creation_input_tokens": 3000,
        }));
        assert_eq!(result["input"], 3000, "only cached_tokens is subtracted");
        assert_eq!(result["input_cached_tokens"], 12000);
        assert_eq!(result["cache_creation_input_tokens"], 3000);
    }

    #[test]
    fn prompt_tokens_details_list_vertex_ai() {
        let result = normalized(json!({
            "prompt_token_count": 1000, "candidates_token_count": 200, "total_token_count": 1200,
            "prompt_tokens_details": [
                {"modality": "text", "token_count": 800},
                {"modality": "image", "token_count": 200},
            ],
        }));
        assert_eq!(
            result,
            json!({
                "input": 0, "output": 200, "total": 1200,
                "input_modality_text": 800, "input_modality_image": 200,
            })
        );
    }

    #[test]
    fn prompt_tokens_details_dict_empty() {
        let result = normalized(json!({
            "prompt_tokens": 5000, "completion_tokens": 100, "total_tokens": 5100,
            "prompt_tokens_details": {},
        }));
        assert_eq!(result, json!({"input": 5000, "output": 100, "total": 5100}));
    }

    #[test]
    fn anthropic_cache_creation_nested_dict() {
        let result = normalized(json!({
            "input_tokens": 9454, "output_tokens": 380, "cache_read_input_tokens": 0,
            "cache_creation": {"ephemeral_1h_input_tokens": 0, "ephemeral_5m_input_tokens": 2048},
        }));
        assert_eq!(
            result,
            json!({
                "input": 9454, "output": 380, "cache_read_input_tokens": 0,
                "cache_creation_ephemeral_1h_input_tokens": 0,
                "cache_creation_ephemeral_5m_input_tokens": 2048,
                "cache_creation_input_tokens": 2048,
            })
        );
    }

    #[test]
    fn anthropic_cache_creation_keeps_existing_aggregate() {
        let result = normalized(json!({
            "input_tokens": 100, "output_tokens": 10, "cache_creation_input_tokens": 3000,
            "cache_creation": {"ephemeral_1h_input_tokens": 1000, "ephemeral_5m_input_tokens": 2000},
        }));
        assert_eq!(result["cache_creation_input_tokens"], 3000);
        assert_eq!(result["cache_creation_ephemeral_1h_input_tokens"], 1000);
        assert_eq!(result["cache_creation_ephemeral_5m_input_tokens"], 2000);
    }

    #[test]
    fn anthropic_cache_creation_all_zero() {
        let result = normalized(json!({
            "input_tokens": 50, "output_tokens": 5,
            "cache_creation": {"ephemeral_1h_input_tokens": 0, "ephemeral_5m_input_tokens": 0},
        }));
        assert_eq!(result["cache_creation_ephemeral_1h_input_tokens"], 0);
        assert_eq!(result["cache_creation_ephemeral_5m_input_tokens"], 0);
        assert!(result.get("cache_creation_input_tokens").is_none());
    }

    #[test]
    fn priority_tier_not_subtracted() {
        let result = normalized(json!({
            "input_tokens": 13, "output_tokens": 1, "total_tokens": 14,
            "input_token_details": {"audio": 0, "priority_cache_read": 0, "priority": 13},
            "output_token_details": {"audio": 0, "priority_reasoning": 0, "priority": 1},
        }));
        assert_eq!(result["input"], 13);
        assert_eq!(result["output"], 1);
        assert_eq!(result["total"], 14);
        assert_eq!(result["input_priority"], 13);
        assert_eq!(result["output_priority"], 1);
    }

    // Further cases.

    #[test]
    fn plain_openai_usage_is_left_to_langfuse() {
        let usage = json!({"prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12});
        assert_eq!(normalized(usage.clone()), usage);
        let usage = json!({
            "prompt_tokens": 10, "completion_tokens": 2, "total_tokens": 12,
            "prompt_tokens_details": {"cached_tokens": 4},
            "completion_tokens_details": {"reasoning_tokens": 1},
        });
        assert_eq!(
            normalized(usage.clone()),
            usage,
            "kept as is, details included"
        );
    }

    #[test]
    fn bedrock_stream_metrics_are_summed() {
        let result = normalized(json!({
            "inputTokenCount": [5, 0, 0], "outputTokenCount": [1, 2, 3], "totalTokenCount": 11,
        }));
        assert_eq!(result, json!({"input": 5, "output": 6, "total": 11}));
    }

    #[test]
    fn vertex_ai_cache_and_candidates_details() {
        let result = normalized(json!({
            "prompt_token_count": 100, "candidates_token_count": 50, "total_token_count": 150,
            "prompt_tokens_details": [{"modality": "TEXT", "token_count": 100}],
            "candidates_tokens_details": [{"modality": "TEXT", "token_count": 50}],
            "cache_tokens_details": [{"modality": "TEXT", "token_count": 40}],
        }));
        assert_eq!(
            result,
            json!({
                "input": 0, "output": 0, "total": 150,
                "input_modality_TEXT": 60, "output_modality_TEXT": 50, "cached_modality_TEXT": 40,
            })
        );
    }

    #[test]
    fn ibm_keys_and_non_integer_values() {
        let result = normalized(json!({
            "input_token_count": 7, "generated_token_count": 3,
            "model": "granite", "ratio": 0.5, "flag": true, "nested": {"a": 1},
        }));
        assert_eq!(result, json!({"input": 7, "output": 3}));
    }

    #[test]
    fn no_usage() {
        assert_eq!(normalize_usage(&json!(null)), None);
        assert_eq!(normalize_usage(&json!([1, 2])), None);
        assert_eq!(normalize_usage(&json!({})), None);
        assert_eq!(normalize_usage(&json!({"model": "x"})), None);
    }

    #[test]
    fn later_translations_win() {
        // `prompt_tokens` comes after `input_tokens` in the translation order.
        let result = normalized(json!({"input_tokens": 1, "prompt_tokens": 2, "output_tokens": 3}));
        assert_eq!(result, json!({"input": 2, "output": 3}));
    }
}
