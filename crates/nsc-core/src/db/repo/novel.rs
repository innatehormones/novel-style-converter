use chrono::{DateTime, Utc};
use rusqlite::{params, Row};

use crate::error::Result;
use crate::models::{NewTransformationNovel, NewUpload, TransformationNovel, Upload};

pub struct UploadRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> UploadRepo<'a> {
    pub fn insert(&self, u: &NewUpload) -> Result<i64> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO uploads (sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![u.sha256, u.filename, u.byte_size, now.to_rfc3339(), u.file_path, u.original_text, u.word_count],
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
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count \
             FROM uploads WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<Upload>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count \
             FROM uploads ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 把原文整篇写回 uploads.original_text(用于清洗/重解析等需要重写原文的路径)。
    /// 同步刷新 word_count:原文变了,字数跟着变,避免 list 显示旧值。
    pub fn set_original_text(&self, id: i64, text: &str) -> Result<()> {
        let wc = crate::text::word_count(text) as i64;
        self.conn.execute(
            "UPDATE uploads SET original_text = ?2, word_count = ?3 WHERE id = ?1",
            params![id, text, wc],
        )?;
        Ok(())
    }

    /// 把 `word_count = 0` 且 `original_text` 非空的 upload 行用真实字符数回填。
    ///
    /// Migration 0007 加 `uploads.word_count` 时给老行填了默认值 0;此函数在
    /// `Db::open` 末尾跑一次,把这些行的 word_count 用已存的 original_text 重算。
    /// 幂等:重跑只触发一次 UPDATE(已经在的字数已正确)。空 original_text 的
    /// 极老 upload 留 0(原文没存进 DB,需要重传才能填)。
    ///
    /// 返回回填的行数(给日志/测试用)。
    pub fn backfill_word_count(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, original_text FROM uploads \
             WHERE word_count = 0 AND length(original_text) > 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0;
        for row in rows {
            let (id, text) = row?;
            let wc = crate::text::word_count(&text) as i64;
            if wc > 0 {
                self.conn.execute(
                    "UPDATE uploads SET word_count = ?2 WHERE id = ?1",
                    params![id, wc],
                )?;
                updated += 1;
            }
        }
        Ok(updated)
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
        word_count: row.get(7)?,
    })
}

pub struct TransformationNovelRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> TransformationNovelRepo<'a> {
    /// 创建 transformation_novel。不再写 `data_assets.locked_at`(该列已废弃)——
    /// 是否被引用看 `transformation_novels` 真实行,前端按钮按 join 出来的
    /// tn_count 走。
    pub fn insert(&self, n: &NewTransformationNovel) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let mode_str = n.default_mode.map(|m| match m {
            crate::models::PromptKind::Compress => "compress",
            crate::models::PromptKind::Style => "style",
        });
        self.conn.execute(
            "INSERT INTO transformation_novels \
             (data_asset_id, title, created_at, default_model_config_id, default_prompt_id, default_mode) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                n.data_asset_id, n.title, now,
                n.default_model_config_id, n.default_prompt_id, mode_str,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at, default_model_config_id, default_prompt_id, default_mode \
             FROM transformation_novels WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(novel_from_row(row)?))
        } else { Ok(None) }
    }

    pub fn list(&self) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at, default_model_config_id, default_prompt_id, default_mode \
             FROM transformation_novels ORDER BY id DESC"
        )?;
        let rows = stmt.query_map([], |row| novel_from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn update(&self, n: &TransformationNovel) -> Result<()> {
        let mode_str = n.default_mode.map(|m| match m {
            crate::models::PromptKind::Compress => "compress",
            crate::models::PromptKind::Style => "style",
        });
        self.conn.execute(
            "UPDATE transformation_novels \
             SET title = ?2, default_model_config_id = ?3, default_prompt_id = ?4, default_mode = ?5 \
             WHERE id = ?1",
            params![
                n.id, n.title,
                n.default_model_config_id, n.default_prompt_id, mode_str,
            ],
        )?;
        Ok(())
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<TransformationNovel>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, title, created_at, default_model_config_id, default_prompt_id, default_mode \
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
    let mode_s: Option<String> = row.get(6)?;
    let default_mode = mode_s.map(|s| match s.as_str() {
        "compress" => Ok(crate::models::PromptKind::Compress),
        "style" => Ok(crate::models::PromptKind::Style),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            6, rusqlite::types::Type::Text,
            format!("unknown default_mode: {other}").into())),
    }).transpose()?;
    Ok(TransformationNovel {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        title: row.get(2)?,
        created_at,
        default_model_config_id: row.get(4)?,
        default_prompt_id: row.get(5)?,
        default_mode,
    })
}