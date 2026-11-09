use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::evaluation_rules::{
    CreateEvaluationRuleRequest, DeletedEvaluationRule, EvaluationRule, UpdateEvaluationRuleRequest,
};
use crate::pagination::{CursorPaginated, CursorParams};

const BASE: &str = "/api/public/v2/evaluation-rules";

/// Evaluation rules. Error bodies follow
/// [`PublicApiError`](crate::models::evaluation_common::PublicApiError).
pub struct EvaluationRulesClient {
    pub(crate) http: Arc<HttpClient>,
}

impl EvaluationRulesClient {
    /// `POST /api/public/v2/evaluation-rules` (201).
    pub async fn create(
        &self,
        req: &CreateEvaluationRuleRequest,
    ) -> Result<EvaluationRule, LangfuseError> {
        self.http.post(BASE, req).await
    }

    /// `GET /api/public/v2/evaluation-rules`, newest first. `limit` defaults to 50 (max 100).
    pub async fn list(
        &self,
        params: &CursorParams,
    ) -> Result<CursorPaginated<EvaluationRule>, LangfuseError> {
        self.http.get_with_query(BASE, params).await
    }

    /// `GET /api/public/v2/evaluation-rules/{evaluationRuleId}`
    pub async fn get(&self, rule_id: &str) -> Result<EvaluationRule, LangfuseError> {
        self.http.get(&rule_path(rule_id)).await
    }

    /// `PATCH /api/public/v2/evaluation-rules/{evaluationRuleId}`
    pub async fn update(
        &self,
        rule_id: &str,
        req: &UpdateEvaluationRuleRequest,
    ) -> Result<EvaluationRule, LangfuseError> {
        self.http.patch(&rule_path(rule_id), req).await
    }

    /// `DELETE /api/public/v2/evaluation-rules/{evaluationRuleId}`. Evaluators and
    /// scores already produced are kept.
    pub async fn delete(&self, rule_id: &str) -> Result<DeletedEvaluationRule, LangfuseError> {
        self.http.delete_with_response(&rule_path(rule_id)).await
    }
}

fn rule_path(rule_id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(rule_id))
}
