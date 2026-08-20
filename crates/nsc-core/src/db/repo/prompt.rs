use std::sync::MutexGuard;
use rusqlite::{params, Connection, Row};

use crate::error::Result;
use crate::models::{Prompt, PromptKind};
use crate::prompts::builtin_prompts;

/// repo 层写时用的"内容更新"结构 —— 不可改 id / is_builtin / archived。
/// 这是 §3.9 的落地:`PromptRepo::update` 不再接受 is_builtin / archived 字段,
/// 杜绝命令层之外的调用者误改。
#[derive(Debug, Clone)]
pub struct PromptUpdate<'a> {
    pub id: i64,
    pub name: &'a str,
    pub kind: PromptKind,
    pub template: &'a str,
}

pub struct PromptRepo<'a> { pub(crate) conn: MutexGuard<'a, Connection> }

impl<'a> PromptRepo<'a> {
    /// 默认列表:仅返回 `archived = 0` 的活动行(UI 主表格)。
    /// `include_archived = true` 时同时返回归档行(用于"显示已归档"切换)。
    pub fn list(&self, include_archived: bool) -> Result<Vec<Prompt>> {
        let sql = if include_archived {
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts ORDER BY archived ASC, id ASC"
        } else {
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts WHERE archived = 0 ORDER BY id ASC"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 按 id 查单行 —— **不**过滤 archived。`BatchScheduler` / `transformation_chapters`
    /// 读 path 必须能拿到归档行,否则历史 tc 引用解析会断。
    pub fn get(&self, id: i64) -> Result<Option<Prompt>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, template, is_builtin, archived \
             FROM prompts WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else { Ok(None) }
    }

    /// insert —— 不带 archived(is_builtin 由调用方传,builtin 种子 1,用户 0)。
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

    /// 更新 name / kind / template —— id / is_builtin / archived **不可改**。
    /// 命令层先 `get` 旧行,再拿 `is_builtin` 拼 PromptUpdate;repo 不做这事。
    pub fn update(&self, u: &PromptUpdate<'_>) -> Result<()> {
        let kind = match u.kind {
            PromptKind::Compress => "compress",
            PromptKind::Style => "style",
        };
        self.conn.execute(
            "UPDATE prompts SET name=?2, kind=?3, template=?4 WHERE id=?1",
            params![u.id, u.name, kind, u.template],
        )?;
        Ok(())
    }

    /// 软删:`archived = 1`。行保留(builtin 行亦可软删 —— 用户可以"归档 builtin 不再用")。
    /// 与 model 不一样的是 prompt 没有密钥,所以只改 archived,不动其他字段。
    pub fn archive(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE prompts SET archived = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 取消软删:恢复 `archived = 0`。
    pub fn restore(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE prompts SET archived = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// 统计 `transformation_chapters` 表里 prompt_id 等于参数的行数。
    /// 删除 prompt 前展示"被 N 个转换结果引用",N=0 不展示。
    pub fn count_by_prompt(&self, prompt_id: i64) -> Result<i64> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM transformation_chapters WHERE prompt_id = ?1",
            params![prompt_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// builtin 种子 —— 在 builtin 行 count = 0 时种入。
    /// 注意:种入条件是 `is_builtin=1 AND archived=0` 的 count = 0;
    /// 用户软删 builtin 后,count 仍 >= 1(archived=1),不再种入。
    /// 这是 §1.3 的 trade-off:用户显式归档 builtin 后,启动不会再"自作主张"种回。
    pub fn seed_builtin_if_empty(&self) -> Result<()> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM prompts WHERE is_builtin = 1 AND archived = 0",
            [],
            |r| r.get(0),
        )?;
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
        archived: row.get(5)?,
    })
}
