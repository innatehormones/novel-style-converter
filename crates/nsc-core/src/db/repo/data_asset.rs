use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::error::Result;
use crate::models::{DataAsset, NewDataAsset};

pub struct DataAssetWithUpload {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub filename: String,
    pub byte_size: i64,
    pub word_count: i64,
    pub tn_count: i64,
}

pub struct DataAssetRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

fn from_row(row: &Row) -> rusqlite::Result<DataAsset> {
    let parsed_at_s: String = row.get(3)?;
    let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    let kind_s: String = row.get(5)?;
    let kind = crate::models::DataAssetKind::parse(&kind_s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(KindParseError(kind_s.clone())))
    })?;

#[derive(Debug)]
struct KindParseError(String);
impl std::fmt::Display for KindParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown kind: {}", self.0)
    }
}
impl std::error::Error for KindParseError {}
    let source_workflow_id: Option<i64> = row.get(6)?;
    let source_data_asset_id: Option<i64> = row.get(7)?;
    let note: String = row.get(8)?;
    Ok(DataAsset {
        id: row.get(0)?,
        upload_id: row.get(1)?,
        title: row.get(2)?,
        parsed_at,
        source_filename: row.get(4)?,
        kind,
        source_workflow_id,
        source_data_asset_id,
        note,
    })
}

impl<'a> DataAssetRepo<'a> {
    pub fn insert(&self, d: &NewDataAsset) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at, source_filename) VALUES (?1, ?2, ?3, ?4)",
            params![d.upload_id, d.title, now, d.source_filename],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets ORDER BY id DESC",
        )?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn find_by_upload(&self, upload_id: i64) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, source_filename, kind, source_workflow_id, source_data_asset_id, note              FROM data_assets WHERE upload_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![upload_id], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_with_upload(&self) -> Result<Vec<DataAssetWithUpload>> {
        let mut stmt = self.conn.prepare(
            "SELECT da.id, da.upload_id, da.title, da.parsed_at,                     COALESCE(u.filename, da.source_filename) AS filename,                     COALESCE(u.byte_size, 0) AS byte_size,                     COALESCE(SUM(c.word_count), 0) AS word_count,                     COALESCE(tn.cnt, 0) AS tn_count              FROM data_assets da              LEFT JOIN uploads u ON u.id = da.upload_id              LEFT JOIN chapters c ON c.data_asset_id = da.id              LEFT JOIN (SELECT data_asset_id, COUNT(*) AS cnt                         FROM transformation_novels GROUP BY data_asset_id) tn                    ON tn.data_asset_id = da.id              GROUP BY da.id              ORDER BY da.id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let parsed_at_s: String = row.get(3)?;
            let parsed_at = DateTime::parse_from_rfc3339(&parsed_at_s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(DataAssetWithUpload {
                id: row.get(0)?,
                upload_id: row.get(1)?,
                title: row.get(2)?,
                parsed_at,
                filename: row.get(4)?,
                byte_size: row.get(5)?,
                word_count: row.get(6)?,
                tn_count: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let exists: bool = self.conn.query_row(
            "SELECT 1 FROM data_assets WHERE id = ?1",
            params![id],
            |_| Ok(true),
        ).optional()?.unwrap_or(false);
        if !exists {
            return Err(crate::error::Error::NotFound(format!("data_asset {} 不存在", id)));
        }
        self.conn.execute("DELETE FROM data_assets WHERE id = ?1", params![id])?;
        Ok(())
    }
}
