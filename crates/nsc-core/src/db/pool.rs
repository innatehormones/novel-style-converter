use std::path::Path;
use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

use super::migrate::SCHEMAS;
use super::repo::{
    AiCallLogRepo, BatchRepo, ChapterRepo, DataAssetRepo, ModelConfigRepo, PromptRepo,
    TransformationChapterRepo, TransformationNovelRepo, UploadRepo, WorkflowResultRepo,
};

/// SQLite 连接包装。`open` / `open_in_memory` 都会按 `migrations/` 顺序跑未应用的 schema 版本。
///
/// **`Db` 是 `Send` 但不是 `Sync`**(`rusqlite::Connection` 内部 `RefCell`)。
/// 调用方不能把 `Arc<Db>` 移入 `tokio::spawn` future、`spawn_blocking` closure,
/// 也不能把它装进 `Box<dyn Transformer>` 等 trait object 通过共享引用跨线程持有。
/// 跨线程 / 跨 future 共享 DB 的标准做法:**按路径重开** ——
/// 捕获 `db_path: PathBuf`,在 worker 内部调 `Db::connect(&path)` 拿 owned `Db`,
/// 操作完即 drop。
#[derive(Debug)]
pub struct Db { pub conn: Connection }

impl Db {
    /// 打开或创建 SQLite 文件,跑未应用的 migrations。父目录需自行 `create_dir_all`。
    pub fn open(path: &Path) -> Result<Self> {
        let db = Self::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        run_schemas(&db.conn)?;
        UploadRepo { conn: &db.conn }.backfill_word_count()?;
        UploadRepo { conn: &db.conn }.recompute_all_word_count()?;
        ChapterRepo { conn: &db.conn }.recompute_all_word_count()?;
        Ok(db)
    }

    /// Open an already initialized database for runtime work.
    ///
    /// Unlike `open`, this does not run schemas or full-table maintenance. Background
    /// workers must use this entry point so opening a connection never performs writes.
    pub fn connect(path: &Path) -> Result<Self> {
        Self::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
    }

    fn open_with_flags(path: &Path, flags: OpenFlags) -> Result<Self> {
        let conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self { conn })
    }

    /// 内存 SQLite,主要给单测用。仍会跑 migrations。
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
    pub fn promotion(&self) -> crate::db::repo::promotion::PromotionRepo<'_> {
        crate::db::repo::promotion::PromotionRepo { conn: &self.conn }
    }
    pub fn batches(&self) -> BatchRepo<'_> { BatchRepo { conn: &self.conn } }
    pub fn workflow_results(&self) -> WorkflowResultRepo<'_> {
        WorkflowResultRepo { conn: &self.conn }
    }

    /// AI 调用日志 repo —— 每次 LLM chat 调用落一行,详见 migrations/0018_ai_call_logs.sql。
    /// recorder (Phase 2) 通过背景 task 调 insert;UI 看板调 list / get / clear。
    pub fn ai_call_logs(&self) -> AiCallLogRepo<'_> {
        AiCallLogRepo { conn: &self.conn }
    }

    pub fn seed_builtin_prompts(&self) -> Result<()> {
        self.prompts().seed_builtin_if_empty()
    }

    
    /// 返回已应用的 schema 版本列表(按 version 升序)。供迁移测试和
    /// 升级诊断用;生产代码不需要直接关心。
    /// 排序按 `LENGTH(version), version`,让 "v10" 排在 "v9" 之后
    /// (纯字符串排序会让 "v10" 排在 "v2" 前,版本号进两位后会撞这个坑)。
    pub fn applied_schema_versions(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT version FROM schema_versions ORDER BY LENGTH(version), version",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Test/admin helper: execute a raw SQL batch. Production code paths
    /// must use the typed repos. Marked `#[doc(hidden)]` to keep it out
    /// of rendered docs but allow internal test access.
    #[doc(hidden)]
    pub fn execute_batch(&self, sql: &str) -> rusqlite::Result<()> {
        self.conn.execute_batch(sql)
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

    #[test]
    fn runtime_connection_rejects_uninitialized_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");

        assert!(Db::connect(&path).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn runtime_connection_opens_while_another_connection_is_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.db");
        let initialized = Db::open(&path).unwrap();
        initialized.conn.execute_batch("BEGIN IMMEDIATE").unwrap();

        let runtime = Db::connect(&path).unwrap();

        assert_eq!(
            runtime.applied_schema_versions().unwrap(),
            initialized.applied_schema_versions().unwrap(),
        );
    }
}
