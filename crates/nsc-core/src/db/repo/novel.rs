use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::Result;
use crate::models::{NewTransformationNovel, NewUpload, TransformationNovel, Upload};

pub struct UploadRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> UploadRepo<'a> {
    pub fn insert(&self, u: &NewUpload) -> Result<i64> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO uploads (sha256, filename, byte_size, uploaded_at, file_path, original_text) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![u.sha256, u.filename, u.byte_size, now.to_rfc3339(), u.file_path, u.original_text],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 同 hash 复用现有 upload。返回 existing id;若不存在返回 None。
    pub fn find_by_sha256(&self, sha256: &str) -> Result<Option<i64>> {
        let mut stmt = self.conn.prepare("SELECT id FROM uploads WHERE sha256 = ?1")?;
        let mut rows = stmt.query(params![sha256])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else { Ok(None) }
    }

    pub fn get(&self, id: i64) -> Result<Option<Upload>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text \
             FROM uploads WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<Upload>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text \
             FROM uploads ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 把原文整篇写回 uploads.original_text(用于清洗/重解析等需要重写原文的路径)。
    pub fn set_original_text(&self, id: i64, text: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE uploads SET original_text = ?2 WHERE id = ?1",
            params![id, text],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM uploads WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<Upload> {
    let uploaded_at_s: String = row.get(4)?;
    let uploaded_at = DateTime::parse_from_rfc3339(&uploaded_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            4, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(Upload {
        id: row.get(0)?,
        sha256: row.get(1)?,
        filename: row.get(2)?,
        byte_size: row.get(3)?,
        uploaded_at,
        file_path: row.get(5)?,
        original_text: row.get(6)?,
    })
}

pub struct TransformationNovelRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> TransformationNovelRepo<'a> {
    /// 创建 transformation_novel 并锁定 data_asset(单事务)。
    /// 失败回滚:transformation_novel 与 data_asset.locked_at 都不会留下脏数据。
    pub fn insert(&self, n: &NewTransformationNovel) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO transformation_novels (data_asset_id, title, created_at) \
             VALUES (?1, ?2, ?3)",
            params![n.data_asset_id, n.title, now],
        )?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "UPDATE data_assets SET locked_at = ?2 WHERE id = ?1",
            params![n.data_asset_id, now],
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get(&self, id: i64) -> Result<Option<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at \
             FROM transformation_novels WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(novel_from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at \
             FROM transformation_novels ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| novel_from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn update(&self, n: &TransformationNovel) -> Result<()> {
        self.conn.execute(
            "UPDATE transformation_novels SET title = ?2 WHERE id = ?1",
            params![n.id, n.title],
        )?;
        Ok(())
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at \
             FROM transformation_novels WHERE data_asset_id = ?1 ORDER BY id DESC"
        )?;
        let rows = stmt.query_map(params![data_asset_id], |row| novel_from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM transformation_novels WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn novel_from_row(row: &Row) -> rusqlite::Result<TransformationNovel> {
    let created_at_s: String = row.get(3)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            3, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(TransformationNovel {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        title: row.get(2)?,
        created_at,
    })
}