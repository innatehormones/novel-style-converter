use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataAsset {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct NewDataAsset {
    pub upload_id: i64,
    pub title: String,
}