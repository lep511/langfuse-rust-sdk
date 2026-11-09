//! Dashboards (unstable API, feature `unstable`).

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::unstable_dashboards::{
    CreateDashboardPlacementRequest, CreateDashboardRequest, Dashboard, DashboardPlacement,
    DeleteDashboardPlacementResponse, DeleteDashboardResponse, UpdateDashboardPlacementRequest,
    UpdateDashboardRequest,
};
use crate::pagination::{PageParams, Paginated};

const BASE: &str = "/api/public/unstable/dashboards";

/// Dashboards and their widget placements. Error bodies follow
/// [`UnstablePublicApiError`](crate::models::unstable_common::UnstablePublicApiError).
pub struct UnstableDashboardsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl UnstableDashboardsClient {
    /// `GET /api/public/unstable/dashboards`, most recently updated first.
    pub async fn list(&self, params: &PageParams) -> Result<Paginated<Dashboard>, LangfuseError> {
        self.http.get_with_query(BASE, params).await
    }

    /// `POST /api/public/unstable/dashboards`
    pub async fn create(&self, req: &CreateDashboardRequest) -> Result<Dashboard, LangfuseError> {
        self.http.post(BASE, req).await
    }

    /// `GET /api/public/unstable/dashboards/{dashboardId}`
    pub async fn get(&self, dashboard_id: &str) -> Result<Dashboard, LangfuseError> {
        self.http.get(&dashboard_path(dashboard_id)).await
    }

    /// `PATCH /api/public/unstable/dashboards/{dashboardId}`
    pub async fn update(
        &self,
        dashboard_id: &str,
        req: &UpdateDashboardRequest,
    ) -> Result<Dashboard, LangfuseError> {
        self.http.patch(&dashboard_path(dashboard_id), req).await
    }

    /// `DELETE /api/public/unstable/dashboards/{dashboardId}`
    pub async fn delete(
        &self,
        dashboard_id: &str,
    ) -> Result<DeleteDashboardResponse, LangfuseError> {
        self.http
            .delete_with_response(&dashboard_path(dashboard_id))
            .await
    }

    /// `POST /api/public/unstable/dashboards/{dashboardId}/placements`. Returns
    /// [`LangfuseError::Conflict`] if a placement with the same id exists.
    pub async fn add_placement(
        &self,
        dashboard_id: &str,
        req: &CreateDashboardPlacementRequest,
    ) -> Result<DashboardPlacement, LangfuseError> {
        self.http
            .post(&format!("{}/placements", dashboard_path(dashboard_id)), req)
            .await
    }

    /// `PATCH /api/public/unstable/dashboards/{dashboardId}/placements/{placementId}`
    pub async fn update_placement(
        &self,
        dashboard_id: &str,
        placement_id: &str,
        req: &UpdateDashboardPlacementRequest,
    ) -> Result<DashboardPlacement, LangfuseError> {
        self.http
            .patch(&placement_path(dashboard_id, placement_id), req)
            .await
    }

    /// `DELETE /api/public/unstable/dashboards/{dashboardId}/placements/{placementId}`:
    /// removes the tile; the widget itself is kept.
    pub async fn delete_placement(
        &self,
        dashboard_id: &str,
        placement_id: &str,
    ) -> Result<DeleteDashboardPlacementResponse, LangfuseError> {
        self.http
            .delete_with_response(&placement_path(dashboard_id, placement_id))
            .await
    }
}

fn dashboard_path(dashboard_id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(dashboard_id))
}

fn placement_path(dashboard_id: &str, placement_id: &str) -> String {
    format!(
        "{}/placements/{}",
        dashboard_path(dashboard_id),
        encode_path_segment(placement_id)
    )
}
