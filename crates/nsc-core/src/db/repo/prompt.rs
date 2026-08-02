use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Prompt, PromptKind};
use crate::prompts::builtin_prompts;

pub struct PromptRepo<'a> { pub(crate) conn: &'a Connection }

impl<'a> PromptRepo<'a> {
    pub fn list(&self) -> Result<Vec<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, template, is_builtin FROM prompts ORDER BY id ASC"
        )?;
        let rows = stmt.query_map([], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: i64) -> Result<Option<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, template, is_builtin FROM prompts WHERE id = ?1"
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    pub fn insert(&self, p: &Prompt) -> Result<i64> {
        let kind = match p.kind {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "INSERT INTO prompts (name, kind, template, is_builtin) VALUES (?1, ?2, ?3, ?4)",
            params![p.name, kind, p.template, p.is_builtin as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update(&self, p: &Prompt) -> Result<()> {
        let kind = match p.kind {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "UPDATE prompts SET name=?2, kind=?3, template=?4, is_builtin=?5 WHERE id=?1",
            params![p.id, p.name, kind, p.template, p.is_builtin as i64],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM prompts WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 统计 `transformation_chapters` 表里 prompt_id 等于参数的行数。
    /// 删除 prompt 前给用户展示"被 N 个转换结果引用"用。
    pub fn count_by_prompt(&self, prompt_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE prompt_id = ?1",
            params![prompt_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn seed_builtin_if_empty(&self) -> Result<()> {
        let count: i64 = self.conn
            .query_row("SELECT COUNT(*) FROM prompts WHERE is_builtin = 1", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        for bp in builtin_prompts() {
            let kind = match bp.kind {
                PromptKind::Compress => "compress",
                PromptKind::Style => "style",
            };
            tx.execute(
                "INSERT INTO prompts (name, kind, template, is_builtin) VALUES (?1, ?2, ?3, 1)",
                params![bp.name, kind, bp.template],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

fn from_row(row: &Row) -> rusqlite::Result<Prompt> {
    let kind_s: String = row.get(2)?;
    let is_builtin: i64 = row.get(4)?;
    Ok(Prompt {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: match kind_s.as_str() {
            "compress" => PromptKind::Compress,
            _ => PromptKind::Style,
        },
        template: row.get(3)?,
        is_builtin: is_builtin != 0,
    })
}
