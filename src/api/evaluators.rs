//! Manage evaluator definitions and versions.

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::evaluators::{
    CreateEvaluatorRequest, DeletedEvaluator, Evaluator, EvaluatorVersion, UpdateEvaluatorRequest,
};
use crate::pagination::{CursorPaginated, CursorParams};

const BASE: &str = "/api/public/v2/evaluators";

/// Evaluators. Error bodies follow
/// [`PublicApiError`](crate::models::evaluation_common::PublicApiError).
pub struct EvaluatorsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl EvaluatorsClient {
    /// `POST /api/public/v2/evaluators` (201).
    pub async fn create(&self, req: &CreateEvaluatorRequest) -> Result<Evaluator, LangfuseError> {
        self.http.post(BASE, req).await
    }

    /// `GET /api/public/v2/evaluators`, newest first. `limit` defaults to 50 (max 100).
    pub async fn list(
        &self,
        params: &CursorParams,
    ) -> Result<CursorPaginated<Evaluator>, LangfuseError> {
        self.http.get_with_query(BASE, params).await
    }

    /// `GET /api/public/v2/evaluators/{evaluatorId}`
    pub async fn get(&self, evaluator_id: &str) -> Result<Evaluator, LangfuseError> {
        self.http.get(&evaluator_path(evaluator_id)).await
    }

    /// `PATCH /api/public/v2/evaluators/{evaluatorId}`
    pub async fn update(
        &self,
        evaluator_id: &str,
        req: &UpdateEvaluatorRequest,
    ) -> Result<Evaluator, LangfuseError> {
        self.http.patch(&evaluator_path(evaluator_id), req).await
    }

    /// `DELETE /api/public/v2/evaluators/{evaluatorId}`: deletes every version and
    /// its rule assignments. Scores already produced are kept.
    pub async fn delete(&self, evaluator_id: &str) -> Result<DeletedEvaluator, LangfuseError> {
        self.http
            .delete_with_response(&evaluator_path(evaluator_id))
            .await
    }

    /// `GET /api/public/v2/evaluators/{evaluatorId}/versions`, newest first.
    pub async fn list_versions(
        &self,
        evaluator_id: &str,
        params: &CursorParams,
    ) -> Result<CursorPaginated<EvaluatorVersion>, LangfuseError> {
        self.http
            .get_with_query(
                &format!("{}/versions", evaluator_path(evaluator_id)),
                params,
            )
            .await
    }
}

fn evaluator_path(evaluator_id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(evaluator_id))
}
