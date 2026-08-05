use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::error::Result;
use crate::models::{DataAsset, NewDataAsset};

/// State 2 列表视图:data_asset + 来源 upload 文件名 + 章节总字数 + 引用此
/// data_asset 的 transformation_novel 计数。前端 Library.vue 表格用。
/// `tn_count` 走 `LEFT JOIN transformation_novels GROUP BY da.id` 实时统计,
/// 不读 `data_assets.locked_at`(该列已被废弃 —— 早先误用来表达"是否被引用",
/// 但 TN 删除时不会主动清,数据会留下历史值,跟真实引用状态脱节)。
pub struct DataAssetWithUpload {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub filename: String,
    pub byte_size: i64,
    /// SUM(chapters.word_count) WHERE data_asset_id = da.id。0 表示尚无章节。
    pub word_count: i64,
    /// COUNT(transformation_novels.id) WHERE data_asset_id = da.id。
    /// 0 表示尚无工作区引用此 data_asset。前端按钮禁用按这个走。
    pub tn_count: i64,
}

pub struct DataAssetRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> DataAssetRepo<'a> {
    pub fn insert(&self, d: &NewDataAsset) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at) VALUES (?1, ?2, ?3)",
            params![d.upload_id, d.title, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at \
             FROM data_assets WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at \
             FROM data_assets ORDER BY id DESC")?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn find_by_upload(&self, upload_id: i64) -> Result<Option<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at \
             FROM data_assets WHERE upload_id = ?1")?;
        let mut rows = stmt.query(params![upload_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list_with_upload(&self) -> Result<Vec<DataAssetWithUpload>> {
        // 聚合章节总字数 + 工作区引用计数。LEFT JOIN + COALESCE 处理空集合(0)。
        // 一次性 GROUP BY 避免 N+1。
        let mut stmt = self.conn.prepare(
            "SELECT da.id, da.upload_id, da.title, da.parsed_at, \
                    u.filename, u.byte_size, \
                    COALESCE(SUM(c.word_count), 0) AS word_count, \
                    COALESCE(tn.cnt, 0) AS tn_count \
             FROM data_assets da \
             JOIN uploads u ON u.id = da.upload_id \
             LEFT JOIN chapters c ON c.data_asset_id = da.id \
             LEFT JOIN (SELECT data_asset_id, COUNT(*) AS cnt \
                        FROM transformation_novels GROUP BY data_asset_id) tn \
                   ON tn.data_asset_id = da.id \
             GROUP BY da.id \
             ORDER BY da.id DESC")?;
        let rows = stmt.query_map([], |row| {
            let parsed_at: String = row.get(3)?;
            let parsed_at = DateTime::parse_from_rfc3339(&parsed_at)
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
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
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

fn from_row(row: &Row) -> rusqlite::Result<DataAsset> {
    let parsed_at = DateTime::parse_from_rfc3339(row.get::<_, String>(3)?.as_str())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(DataAsset {
        id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?,
        parsed_at,
    })
}
