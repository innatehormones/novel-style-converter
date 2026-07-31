use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Chapter, NewChapter};

pub struct ChapterRepo<'a> { pub(crate) conn: &'a Connection }

/// 给章节解析页 UI 用的章节段(byte_start/end 允许 NULL 表示老数据)。
#[derive(Debug, Clone)]
pub struct ChapterSegmentRow {
    pub id: i64,
    pub idx: i32,
    pub title: String,
    pub byte_start: Option<i64>,
    pub byte_end: Option<i64>,
    pub word_count: i32,
}

fn segment_from_row(row: &Row<'_>) -> rusqlite::Result<ChapterSegmentRow> {
    Ok(ChapterSegmentRow {
        id: row.get(0)?,
        idx: row.get(1)?,
        title: row.get(2)?,
        byte_start: row.get(3)?,
        byte_end: row.get(4)?,
        word_count: row.get(5)?,
    })
}

impl<'a> ChapterRepo<'a> {
    pub fn insert(&self, c: &NewChapter) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO chapters (data_asset_id, idx, title, byte_start, byte_end, word_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![c.data_asset_id, c.idx, c.title, c.byte_start, c.byte_end, c.word_count],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_batch(&self, items: &[NewChapter]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "INSERT INTO chapters (data_asset_id, idx, title, byte_start, byte_end, word_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for c in items {
            stmt.execute(params![c.data_asset_id, c.idx, c.title, c.byte_start, c.byte_end, c.word_count])?;
        }
        drop(stmt);
        tx.commit()?;
        Ok(())
    }

    /// 一次性插入一批章节(同一 data_asset)。data_asset_id 由调用方提供,
    /// NewChapter 字段里不带 data_asset_id(便于直接用 splitter 输出的字节偏移)。
    pub fn insert_many(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chapters (data_asset_id, idx, title, byte_start, byte_end, word_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for c in items {
                stmt.execute(params![data_asset_id, c.idx, c.title, c.byte_start, c.byte_end, c.word_count])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// 从 chapters 表读已提交章节,按 idx ASC。
    pub fn list_segments_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<ChapterSegmentRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, idx, title, byte_start, byte_end, word_count \
             FROM chapters WHERE data_asset_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![data_asset_id], segment_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, byte_start, byte_end, word_count \
             FROM chapters WHERE data_asset_id = ?1 ORDER BY idx ASC"
        )?;
        let rows = stmt.query_map(params![data_asset_id], |row| {
            Ok(Chapter {
                id: row.get(0)?,
                data_asset_id: row.get(1)?,
                idx: row.get(2)?,
                title: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                word_count: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, byte_start, byte_end, word_count \
             FROM chapters WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Chapter {
                id: row.get(0)?,
                data_asset_id: row.get(1)?,
                idx: row.get(2)?,
                title: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                word_count: row.get(6)?,
            }))
        } else { Ok(None) }
    }

    /// 单事务内 delete + insert + renumber。失败回滚,旧章节完整保留。
    /// 解析章节时调用:data_asset 还未锁死时(chapters 可被替换),否则应拒绝。
    pub fn replace_all_for_data_asset(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM chapters WHERE data_asset_id = ?1", params![data_asset_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chapters (data_asset_id, idx, title, byte_start, byte_end, word_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for c in items {
                stmt.execute(params![c.data_asset_id, c.idx, c.title, c.byte_start, c.byte_end, c.word_count])?;
            }
        }
        // renumber 在同一事务内:按 idx ASC, id ASC 把当前(新插入)的章节 idx 拍成 1..N
        {
            let mut select_stmt = tx.prepare(
                "SELECT id FROM chapters WHERE data_asset_id = ?1 \
                 ORDER BY idx ASC, id ASC",
            )?;
            let ids: Vec<i64> = select_stmt
                .query_map(params![data_asset_id], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(select_stmt);
            let mut update_stmt =
                tx.prepare("UPDATE chapters SET idx = ?2 WHERE id = ?1")?;
            for (i, id) in ids.iter().enumerate() {
                update_stmt.execute(params![id, (i + 1) as i32])?;
            }
        }
        tx.commit()?;
        Ok(items.len())
    }

    pub fn prev_n(&self, data_asset_id: i64, before_idx: i32, n: i32) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, byte_start, byte_end, word_count \
             FROM chapters WHERE data_asset_id = ?1 AND idx < ?2 \
             ORDER BY idx DESC LIMIT ?3"
        )?;
        let rows = stmt.query_map(params![data_asset_id, before_idx, n], |row| {
            Ok(Chapter {
                id: row.get(0)?,
                data_asset_id: row.get(1)?,
                idx: row.get(2)?,
                title: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                word_count: row.get(6)?,
            })
        })?;
        let mut v: Vec<Chapter> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    pub fn next_n(&self, data_asset_id: i64, after_idx: i32, n: i32) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, byte_start, byte_end, word_count \
             FROM chapters WHERE data_asset_id = ?1 AND idx > ?2 \
             ORDER BY idx ASC LIMIT ?3"
        )?;
        let rows = stmt.query_map(params![data_asset_id, after_idx, n], |row| {
            Ok(Chapter {
                id: row.get(0)?,
                data_asset_id: row.get(1)?,
                idx: row.get(2)?,
                title: row.get(3)?,
                byte_start: row.get(4)?,
                byte_end: row.get(5)?,
                word_count: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}