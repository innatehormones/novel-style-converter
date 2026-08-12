use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAsset {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub source_filename: String,
    #[serde(default = "default_kind_source")]
    pub kind: crate::models::DataAssetKind,
    #[serde(default)]
    pub source_workflow_id: Option<i64>,
    #[serde(default)]
    pub source_data_asset_id: Option<i64>,
    #[serde(default)]
    pub note: String,
}

fn default_kind_source() -> crate::models::DataAssetKind {
    crate::models::DataAssetKind::Source
}

#[derive(Debug, Clone)]
pub struct NewDataAsset {
    pub upload_id: i64,
    pub title: String,
    pub source_filename: String,
}
