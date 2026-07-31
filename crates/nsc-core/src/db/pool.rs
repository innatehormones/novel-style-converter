use std::path::Path;
use rusqlite::{Connection, OpenFlags};

use crate::error::Result;
use crate::models::default_from_env;

use super::migrate::SCHEMAS;
use super::repo::{
    ChapterRepo, DataAssetRepo, ModelConfigRepo, PromptRepo, TransformationChapterRepo,
    TransformationNovelRepo, UploadRepo,
};

#[derive(Debug)]
pub struct Db { conn: Connection }

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        run_schemas(&conn)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        run_schemas(&conn)?;
        Ok(Self { conn })
    }

    pub fn uploads(&self) -> UploadRepo<'_> { UploadRepo { conn: &self.conn } }
    pub fn chapters(&self) -> ChapterRepo<'_> { ChapterRepo { conn: &self.conn } }
    pub fn transformation_novels(&self) -> TransformationNovelRepo<'_> {
        TransformationNovelRepo { conn: &self.conn }
    }
    pub fn transformation_chapters(&self) -> TransformationChapterRepo<'_> {
        TransformationChapterRepo { conn: &self.conn }
    }
    pub fn prompts(&self) -> PromptRepo<'_> { PromptRepo { conn: &self.conn } }
    pub fn model_configs(&self) -> ModelConfigRepo<'_> { ModelConfigRepo { conn: &self.conn } }
    pub fn data_assets(&self) -> DataAssetRepo<'_> { DataAssetRepo { conn: &self.conn } }

    pub fn seed_builtin_prompts(&self) -> Result<()> {
        self.prompts().seed_builtin_if_empty()
    }

    /// 从环境变量读兜底模型并写入空表。表非空则跳过,任一 env 缺失则跳过(不报错)。
    pub fn seed_default_model_from_env(&self) -> Result<Option<i64>> {
        let Some(seed) = default_from_env() else { return Ok(None) };
        self.model_configs().seed_default_if_empty(&seed)
    }

    /// 返回已应用的 schema 版本列表(按 version 升序)。供迁移测试和
    /// 升级诊断用;生产代码不需要直接关心。
    pub fn applied_schema_versions(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT version FROM schema_versions ORDER BY version",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

fn run_schemas(conn: &Connection) -> Result<()> {
    // 记录已应用的版本,只跑未跑过的;否则旧 0001 在 0002 drop 后再 CREATE INDEX
    // 会因 schema 已变而报错。
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )?;
    let now = chrono::Utc::now().to_rfc3339();
    for (version, sql) in SCHEMAS {
        let applied: bool = conn.query_row(
            "SELECT 1 FROM schema_versions WHERE version = ?1",
            [version],
            |_| Ok(true),
        ).unwrap_or(false);
        if applied {
            continue;
        }
        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opens_in_memory_and_seeds_schema() {
        let db = Db::open_in_memory().unwrap();
        let _uploads = db.uploads();
    }
}