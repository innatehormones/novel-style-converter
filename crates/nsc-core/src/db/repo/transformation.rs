use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{
    NewTransformationChapter, TransformationChapter, TransformMode, TransformStatus,
};

pub struct TransformationChapterRepo<'a> { pub(crate) conn: &'a Connection }

impl<'a> TransformationChapterRepo<'a> {
    pub fn insert(&self, t: &NewTransformationChapter) -> Result<i64> {
        let mode = match t.mode {
            TransformMode::Compress => "compress",
            TransformMode::Style => "style",
        };
        self.conn.execute(
            "INSERT INTO transformation_chapters \
             (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, status) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
            params![
                t.transformation_novel_id, t.chapter_id, mode, t.prompt_id, t.model_config_id,
                t.ctx_prev_original, t.ctx_prev_transformed, t.ctx_next_original,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<TransformationChapter>> {
        let sql = format!("{SELECT_SQL} WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    /// 同一章节的所有转换(历史全留,按 id desc)。
    pub fn list_by_chapter(&self, chapter_id: i64) -> Result<Vec<TransformationChapter>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE chapter_id = ?1 ORDER BY id DESC"
        ))?;
        let rows = stmt.query_map(params![chapter_id], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 同一 transformation_novel 的所有转换。
    pub fn list_by_transformation_novel(
        &self,
        transformation_novel_id: i64,
    ) -> Result<Vec<TransformationChapter>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE transformation_novel_id = ?1 ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map(params![transformation_novel_id], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_status(&self, status: TransformStatus) -> Result<Vec<TransformationChapter>> {
        let s = status_str(status);
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_SQL} WHERE status = ?1 ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map(params![s], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn mark_running(&self, id: i64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters SET status='running', started_at=?2 WHERE id=?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn mark_done(&self, id: i64, result_content: String, tokens_in: i32, tokens_out: i32) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='done', result_content=?2, tokens_in=?3, tokens_out=?4, completed_at=?5 \
             WHERE id=?1",
            params![id, result_content, tokens_in, tokens_out, now],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, id: i64, error: String) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='failed', error=?2, completed_at=?3 WHERE id=?1",
            params![id, error, now],
        )?;
        Ok(())
    }

    pub fn reset_to_pending(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE transformation_chapters \
             SET status='pending', result_content=NULL, tokens_in=NULL, tokens_out=NULL, \
                 error=NULL, started_at=NULL, completed_at=NULL \
             WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }
}

const SELECT_SQL: &str =
    "SELECT id, transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
            ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
            status, result_content, tokens_in, tokens_out, \
            error, started_at, completed_at \
     FROM transformation_chapters";

fn parse_ts(idx: usize, s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            idx, rusqlite::types::Type::Text, Box::new(e)))
}

fn from_row(row: &Row) -> rusqlite::Result<TransformationChapter> {
    let mode_s: String = row.get(3)?;
    let status_s: String = row.get(9)?;
    let started: Option<String> = row.get(14)?;
    let completed: Option<String> = row.get(15)?;
    Ok(TransformationChapter {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        chapter_id: row.get(2)?,
        mode: match mode_s.as_str() {
            "compress" => TransformMode::Compress,
            _ => TransformMode::Style,
        },
        prompt_id: row.get(4)?,
        model_config_id: row.get(5)?,
        ctx_prev_original: row.get(6)?,
        ctx_prev_transformed: row.get(7)?,
        ctx_next_original: row.get(8)?,
        status: match status_s.as_str() {
            "pending" => TransformStatus::Pending,
            "running" => TransformStatus::Running,
            "done" => TransformStatus::Done,
            "failed" => TransformStatus::Failed,
            _ => TransformStatus::Cancelled,
        },
        result_content: row.get(10)?,
        tokens_in: row.get(11)?,
        tokens_out: row.get(12)?,
        error: row.get(13)?,
        started_at: started.as_deref().map(|s| parse_ts(14, s)).transpose()?,
        completed_at: completed.as_deref().map(|s| parse_ts(15, s)).transpose()?,
    })
}

fn status_str(s: TransformStatus) -> &'static str {
    match s {
        TransformStatus::Pending => "pending",
        TransformStatus::Running => "running",
        TransformStatus::Done => "done",
        TransformStatus::Failed => "failed",
        TransformStatus::Cancelled => "cancelled",
    }
}