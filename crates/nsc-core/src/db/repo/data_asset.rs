use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::error::Result;
use crate::models::{DataAsset, NewDataAsset};

/// State 2 列表视图:data_asset + 来源 upload 文件名 + 章节总字数,前端 Library.vue 表格用。
pub struct DataAssetWithUpload {
    pub id: i64,
    pub upload_id: i64,
    pub title: String,
    pub parsed_at: DateTime<Utc>,
    pub locked_at: Option<DateTime<Utc>>,
    pub filename: String,
    pub byte_size: i64,
    /// SUM(chapters.word_count) WHERE data_asset_id = da.id。0 表示尚无章节。
    pub word_count: i64,
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
            "SELECT id, upload_id, title, parsed_at, locked_at \
             FROM data_assets WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, locked_at \
             FROM data_assets ORDER BY id DESC")?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn find_by_upload(&self, upload_id: i64) -> Result<Option<DataAsset>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, upload_id, title, parsed_at, locked_at \
             FROM data_assets WHERE upload_id = ?1")?;
        let mut rows = stmt.query(params![upload_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list_with_upload(&self) -> Result<Vec<DataAssetWithUpload>> {
        // 聚合每本 data_asset 的章节总字数:LEFT JOIN + COALESCE 处理没有章节的行(0)。
        let mut stmt = self.conn.prepare(
            "SELECT da.id, da.upload_id, da.title, da.parsed_at, da.locked_at, \
                    u.filename, u.byte_size, \
                    COALESCE(SUM(c.word_count), 0) AS word_count \
             FROM data_assets da \
             JOIN uploads u ON u.id = da.upload_id \
             LEFT JOIN chapters c ON c.data_asset_id = da.id \
             GROUP BY da.id \
             ORDER BY da.id DESC")?;
        let rows = stmt.query_map([], |row| {
            let parsed_at: String = row.get(3)?;
            let parsed_at = DateTime::parse_from_rfc3339(&parsed_at)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
            let locked_at: Option<String> = row.get(4)?;
            let locked_at = locked_at.map(|s| DateTime::parse_from_rfc3339(&s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))))
                .transpose()?;
            Ok(DataAssetWithUpload {
                id: row.get(0)?,
                upload_id: row.get(1)?,
                title: row.get(2)?,
                parsed_at,
                locked_at,
                filename: row.get(5)?,
                byte_size: row.get(6)?,
                word_count: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn is_locked(&self, id: i64) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT locked_at IS NOT NULL FROM data_assets WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => {
                let v: i64 = row.get(0)?;
                Ok(v != 0)
            }
            None => Ok(false),
        }
    }

    pub fn set_locked(&self, id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE data_assets SET locked_at = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn delete_if_unlocked(&self, id: i64) -> Result<()> {
        let locked: Option<Option<String>> = self.conn.query_row(
            "SELECT locked_at FROM data_assets WHERE id = ?1",
            params![id],
            |row| row.get::<_, Option<String>>(0),
        ).optional()?;
        match locked {
            None => return Err(crate::error::Error::NotFound(format!("data_asset {} 不存在", id))),
            Some(Some(_)) => return Err(crate::error::Error::Validation("data_asset 已锁定,无法删除".into())),
            Some(None) => {} // 未锁定,可以删除
        }
        self.conn.execute("DELETE FROM data_assets WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<DataAsset> {
    let parsed_at = DateTime::parse_from_rfc3339(row.get::<_, String>(3)?.as_str())
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?;
    let locked_at: Option<String> = row.get(4)?;
    Ok(DataAsset {
        id: row.get(0)?, upload_id: row.get(1)?, title: row.get(2)?,
        parsed_at,
        locked_at: locked_at.as_deref()
            .map(|s| DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))))
            .transpose()?,
    })
}