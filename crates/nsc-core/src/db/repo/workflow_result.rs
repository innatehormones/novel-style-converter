use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::workflow_result::{WorkflowResult, WorkflowResultChapter};

pub struct WorkflowResultRepo<'a> { pub(crate) conn: &'a Connection }

impl<'a> WorkflowResultRepo<'a> {
    /// 在同一事务内创建结果集 + N 个空结果槽;任一失败回滚。
    /// `INSERT OR IGNORE` 保证对同一 batch 重复调用也是幂等的——Task 3 启动时
    /// 偶发重试路径会撞这里,这里靠 schema 上的 UNIQUE(batch_id) 收口。
    pub fn create_for_batch_with_slots(
        &self,
        batch_id: i64,
        chapter_ids: &[i64],
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "INSERT OR IGNORE INTO workflow_results (batch_id, created_at) VALUES (?1, ?2)",
            params![batch_id, now],
        )?;
        let result_id: i64 = tx.query_row(
            "SELECT id FROM workflow_results WHERE batch_id = ?1",
            params![batch_id], |r| r.get(0),
        )?;
        for cid in chapter_ids {
            tx.execute(
                "INSERT OR IGNORE INTO workflow_result_chapters \
                 (workflow_result_id, chapter_id, content, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, ?3)",
                params![result_id, cid, now],
            )?;
        }
        tx.commit()?;
        Ok(result_id)
    }

    pub fn get_by_batch(&self, batch_id: i64) -> Result<Option<WorkflowResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, batch_id, created_at FROM workflow_results WHERE batch_id = ?1",
        )?;
        let mut rows = stmt.query(params![batch_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_result(row)?))
        } else { Ok(None) }
    }

    pub fn list_chapters(&self, result_id: i64) -> Result<Vec<WorkflowResultChapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, workflow_result_id, chapter_id, content, created_at, updated_at \
             FROM workflow_result_chapters WHERE workflow_result_id = ?1 ORDER BY chapter_id ASC",
        )?;
        let rows = stmt.query_map(params![result_id], |r| row_to_chapter(r))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 按结果槽 id 写 content;id 不存在时静默 noop(避免干扰 worker 重试路径)。
    pub fn write_content(&self, slot_id: i64, content: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE workflow_result_chapters SET content = ?2, updated_at = ?3 WHERE id = ?1",
            params![slot_id, content, now],
        )?;
        Ok(())
    }
}

fn row_to_result(row: &Row<'_>) -> rusqlite::Result<WorkflowResult> {
    let created: String = row.get(2)?;
    let dt = DateTime::parse_from_rfc3339(&created)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(WorkflowResult { id: row.get(0)?, batch_id: row.get(1)?, created_at: dt })
}

fn row_to_chapter(row: &Row<'_>) -> rusqlite::Result<WorkflowResultChapter> {
    let created: String = row.get(4)?;
    let updated: String = row.get(5)?;
    let parse = |s: String, idx: usize| DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e)));
    Ok(WorkflowResultChapter {
        id: row.get(0)?,
        workflow_result_id: row.get(1)?,
        chapter_id: row.get(2)?,
        content: row.get(3)?,
        created_at: parse(created, 4)?,
        updated_at: parse(updated, 5)?,
    })
}
