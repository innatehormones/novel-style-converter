use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Chapter, NewChapter};

pub struct ChapterRepo<'a> { pub(crate) conn: &'a Connection }

fn chapter_from_row(row: &Row<'_>) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        data_asset_id: row.get(1)?,
        idx: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        word_count: row.get(5)?,
    })
}

impl<'a> ChapterRepo<'a> {
    pub fn insert(&self, c: &NewChapter) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO chapters (data_asset_id, idx, title, body, word_count)              VALUES (?1, ?2, ?3, ?4, ?5)",
            params![c.data_asset_id, c.idx, c.title, c.body, c.word_count],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_many(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chapters (data_asset_id, idx, title, body, word_count)                  VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for c in items {
                stmt.execute(params![data_asset_id, c.idx, c.title, c.body, c.word_count])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_by_data_asset(&self, data_asset_id: i64) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count              FROM chapters WHERE data_asset_id = ?1 ORDER BY idx ASC",
        )?;
        let rows = stmt.query_map(params![data_asset_id], chapter_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count              FROM chapters WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(chapter_from_row(row)?))
        } else { Ok(None) }
    }

    pub fn replace_all_for_data_asset(&self, data_asset_id: i64, items: &[NewChapter]) -> Result<usize> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM chapters WHERE data_asset_id = ?1", params![data_asset_id])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO chapters (data_asset_id, idx, title, body, word_count)                  VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for c in items {
                stmt.execute(params![data_asset_id, c.idx, c.title, c.body, c.word_count])?;
            }
        }
        {
            let mut select_stmt = tx.prepare(
                "SELECT id FROM chapters WHERE data_asset_id = ?1                  ORDER BY idx ASC, id ASC",
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
            "SELECT id, data_asset_id, idx, title, body, word_count              FROM chapters WHERE data_asset_id = ?1 AND idx < ?2              ORDER BY idx DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![data_asset_id, before_idx, n], chapter_from_row)?;
        let mut v: Vec<Chapter> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        v.reverse();
        Ok(v)
    }

    pub fn next_n(&self, data_asset_id: i64, after_idx: i32, n: i32) -> Result<Vec<Chapter>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, data_asset_id, idx, title, body, word_count              FROM chapters WHERE data_asset_id = ?1 AND idx > ?2              ORDER BY idx ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![data_asset_id, after_idx, n], chapter_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}
