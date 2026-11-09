//! Dashboard widgets (unstable API, feature `unstable`).

use std::sync::Arc;

use crate::error::LangfuseError;
use crate::http::{HttpClient, encode_path_segment};
use crate::models::unstable_dashboard_widgets::{
    CreateDashboardWidgetRequest, DashboardWidget, DeleteDashboardWidgetResponse,
    UpdateDashboardWidgetRequest,
};
use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};

const BASE: &str = "/api/public/unstable/dashboard-widgets";

/// Standalone chart definitions that can be placed on dashboards.
/// Error bodies follow [`UnstablePublicApiError`](crate::models::unstable_common::UnstablePublicApiError).
#[derive(Clone)]
pub struct UnstableDashboardWidgetsClient {
    pub(crate) http: Arc<HttpClient>,
}

impl UnstableDashboardWidgetsClient {
    /// `GET /api/public/unstable/dashboard-widgets`, most recently updated first.
    pub async fn list(
        &self,
        params: &PageParams,
    ) -> Result<Paginated<DashboardWidget>, LangfuseError> {
        self.http.get_with_query(BASE, params).await
    }

    /// Lazily stream every page of [`list`](Self::list), following `meta.totalPages`;
    /// see [`pagination`](crate::pagination).
    pub fn list_pages(&self, params: PageParams) -> Pages<'_, Paginated<DashboardWidget>> {
        paginate_pages(
            params,
            move |params| async move { self.list(&params).await },
        )
    }

    /// `POST /api/public/unstable/dashboard-widgets`
    pub async fn create(
        &self,
        req: &CreateDashboardWidgetRequest,
    ) -> Result<DashboardWidget, LangfuseError> {
        self.http.post(BASE, req).await
    }

    /// `GET /api/public/unstable/dashboard-widgets/{widgetId}`
    pub async fn get(&self, widget_id: &str) -> Result<DashboardWidget, LangfuseError> {
        self.http.get(&widget_path(widget_id)).await
    }

    /// `PATCH /api/public/unstable/dashboard-widgets/{widgetId}`
    pub async fn update(
        &self,
        widget_id: &str,
        req: &UpdateDashboardWidgetRequest,
    ) -> Result<DashboardWidget, LangfuseError> {
        self.http.patch(&widget_path(widget_id), req).await
    }

    /// `DELETE /api/public/unstable/dashboard-widgets/{widgetId}`. Returns
    /// [`LangfuseError::Conflict`] while the widget is still placed on a dashboard.
    pub async fn delete(
        &self,
        widget_id: &str,
    ) -> Result<DeleteDashboardWidgetResponse, LangfuseError> {
        self.http
            .delete_with_response(&widget_path(widget_id))
            .await
    }
}

fn widget_path(widget_id: &str) -> String {
    format!("{BASE}/{}", encode_path_segment(widget_id))
}
