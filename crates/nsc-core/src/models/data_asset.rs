use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAsset {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub source_filename: String,
}

#[derive(Debug, Clone)]
pub struct NewDataAsset {
    pub upload_id: i64,
    pub title: String,
    pub source_filename: String,
}
