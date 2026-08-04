use chrono::{DateTime, Utc};
use rusqlite::{params, Row};
use serde::Serialize;

use crate::error::Result;
use crate::models::{Batch, BatchStatus, NewBatch, OnFailurePolicy};

pub struct BatchRepo<'a> { pub(crate) conn: &'a rusqlite::Connection }

impl<'a> BatchRepo<'a> {
    /// 插入一条 batch(status='pending')。返回新 id。
    pub fn insert(&self, b: &NewBatch) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let policy_s = policy_to_str(b.on_failure_policy);
        self.conn.execute(
            "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
             VALUES (?1, ?2, ?3, 'pending', ?4)",
            params![b.transformation_novel_id, b.label, policy_s, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get(&self, id: i64) -> Result<Option<Batch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at \
             FROM batches WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? { Ok(Some(batch_from_row(row)?)) } else { Ok(None) }
    }

    pub fn list_by_tn(&self, tn_id: i64) -> Result<Vec<Batch>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at \
             FROM batches WHERE transformation_novel_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![tn_id], |row| batch_from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 设 status 同时自动维护 started_at / ended_at 时间戳。
    /// - Running:started_at 已有则不动,首次写入。
    /// - Completed/Terminated/Cancelled:ended_at 设 NOW。
    /// - 其它:仅改 status。
    pub fn set_status(&self, id: i64, status: BatchStatus) -> Result<()> {
        let status_s = status_to_str(status);
        let now = Utc::now().to_rfc3339();
        match status {
            BatchStatus::Running => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, started_at = COALESCE(started_at, ?3) WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            BatchStatus::Completed | BatchStatus::Terminated | BatchStatus::Cancelled => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2, ended_at = ?3 WHERE id = ?1",
                    params![id, status_s, now],
                )?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE batches SET status = ?2 WHERE id = ?1",
                    params![id, status_s],
                )?;
            }
        }
        Ok(())
    }

    /// 改 label / on_failure_policy。只在 batch 不在 Running 时允许(上层校验)。
    pub fn update(&self, b: &Batch) -> Result<()> {
        let policy_s = policy_to_str(b.on_failure_policy);
        self.conn.execute(
            "UPDATE batches SET label = ?2, on_failure_policy = ?3 WHERE id = ?1",
            params![b.id, b.label, policy_s],
        )?;
        Ok(())
    }

    /// 统计批号各状态计数(给 UI tab badge 用)。
    pub fn count_by_status(&self, tn_id: i64) -> Result<BatchStatusCount> {
        let mut stmt = self.conn.prepare(
            "SELECT status, COUNT(*) FROM batches WHERE transformation_novel_id = ?1 GROUP BY status",
        )?;
        let rows = stmt.query_map(params![tn_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        let mut counts = BatchStatusCount::default();
        for row in rows {
            let (s, n) = row?;
            match s.as_str() {
                "pending" => counts.pending = n,
                "running" => counts.running = n,
                "paused" => counts.paused = n,
                "completed" => counts.completed = n,
                "terminated" => counts.terminated = n,
                "cancelled" => counts.cancelled = n,
                _ => {}
            }
        }
        Ok(counts)
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BatchStatusCount {
    pub pending: i64,
    pub running: i64,
    pub paused: i64,
    pub completed: i64,
    pub terminated: i64,
    pub cancelled: i64,
}

fn batch_from_row(row: &Row) -> rusqlite::Result<Batch> {
    let created_at_s: String = row.get(5)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            5, rusqlite::types::Type::Text, Box::new(e)))?;
    let started_at_s: Option<String> = row.get(6)?;
    let ended_at_s:   Option<String> = row.get(7)?;
    let parse_opt = |s: Option<String>| -> rusqlite::Result<Option<DateTime<Utc>>> {
        match s {
            None => Ok(None),
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|d| Some(d.with_timezone(&Utc)))
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    6, rusqlite::types::Type::Text, Box::new(e))),
        }
    };
    Ok(Batch {
        id: row.get(0)?,
        transformation_novel_id: row.get(1)?,
        label: row.get(2)?,
        on_failure_policy: str_to_policy(&row.get::<_, String>(3)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?,
        status: str_to_status(&row.get::<_, String>(4)?)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?,
        created_at,
        started_at: parse_opt(started_at_s)?,
        ended_at: parse_opt(ended_at_s)?,
    })
}

fn status_to_str(s: BatchStatus) -> &'static str {
    match s {
        BatchStatus::Pending    => "pending",
        BatchStatus::Running    => "running",
        BatchStatus::Paused     => "paused",
        BatchStatus::Completed  => "completed",
        BatchStatus::Terminated => "terminated",
        BatchStatus::Cancelled  => "cancelled",
    }
}
fn str_to_status(s: &str) -> rusqlite::Result<BatchStatus> {
    match s {
        "pending"    => Ok(BatchStatus::Pending),
        "running"    => Ok(BatchStatus::Running),
        "paused"     => Ok(BatchStatus::Paused),
        "completed"  => Ok(BatchStatus::Completed),
        "terminated" => Ok(BatchStatus::Terminated),
        "cancelled"  => Ok(BatchStatus::Cancelled),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown batch status: {other}").into())),
    }
}
fn policy_to_str(p: OnFailurePolicy) -> &'static str {
    match p {
        OnFailurePolicy::PauseAndReview => "pause_and_review",
        OnFailurePolicy::Terminate      => "terminate",
        OnFailurePolicy::SkipFailed     => "skip_failed",
    }
}
fn str_to_policy(s: &str) -> rusqlite::Result<OnFailurePolicy> {
    match s {
        "pause_and_review" => Ok(OnFailurePolicy::PauseAndReview),
        "terminate"        => Ok(OnFailurePolicy::Terminate),
        "skip_failed"      => Ok(OnFailurePolicy::SkipFailed),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0, rusqlite::types::Type::Text,
            format!("unknown on_failure_policy: {other}").into())),
    }
}
