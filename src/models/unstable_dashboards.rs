//! Dashboards (unstable API, feature `unstable`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::unstable_common::DashboardWidgetFilter;

/// A tile on the dashboard's 12-column grid (discriminated by `type`).
/// `x`/`y` are the 0-based top-left cell (`y` grows downward); `width`/`height`
/// are in cells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DashboardPlacement {
    #[serde(rename_all = "camelCase")]
    Widget {
        id: String,
        widget_id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
    /// A Langfuse-managed preset widget.
    #[serde(rename_all = "camelCase")]
    Preset {
        id: String,
        preset_id: String,
        x: i64,
        y: i64,
        width: i64,
        height: i64,
    },
}

/// Body of `POST .../dashboards/{dashboardId}/placements`. Omitted fields get
/// server defaults: a generated id, `x = 0`, `y` below existing tiles, 6x6 size.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CreateDashboardPlacementRequest {
    #[serde(rename_all = "camelCase")]
    Widget {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        widget_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        x: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        y: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Preset {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        preset_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        x: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        y: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<i64>,
    },
}

impl CreateDashboardPlacementRequest {
    /// Place `widget_id` with server-chosen id and position.
    pub fn widget(widget_id: impl Into<String>) -> Self {
        Self::Widget {
            id: None,
            widget_id: widget_id.into(),
            x: None,
            y: None,
            width: None,
            height: None,
        }
    }

    /// Place preset `preset_id` with server-chosen id and position.
    pub fn preset(preset_id: impl Into<String>) -> Self {
        Self::Preset {
            id: None,
            preset_id: preset_id.into(),
            x: None,
            y: None,
            width: None,
            height: None,
        }
    }
}

/// Body of `PATCH .../placements/{placementId}`: move or resize. Set at least one field.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDashboardPlacementRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
}

/// Layout of a dashboard. `widgets` holds placements of both kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardDefinition {
    pub widgets: Vec<DashboardPlacement>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub name: String,
    pub description: String,
    pub definition: DashboardDefinition,
    /// Dashboard-level filters applied to every widget.
    pub filters: Vec<DashboardWidgetFilter>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateDashboardRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<DashboardDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<DashboardWidgetFilter>>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDashboardRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<DashboardDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<DashboardWidgetFilter>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDashboardResponse {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDashboardPlacementResponse {
    pub message: String,
}
