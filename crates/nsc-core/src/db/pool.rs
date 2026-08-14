use std::path::Path;
use rusqlite::{Connection, OpenFlags};

use crate::error::Result;

use super::migrate::SCHEMAS;
use super::repo::{
    AiCallLogRepo, BatchRepo, ChapterPreviewRepo, ChapterRepo, DataAssetRepo,
    ModelConfigRepo, OverviewRepo, PromptRepo, TransformationChapterRepo,
    TransformationNovelRepo, UploadRepo, WorkflowResultRepo,
};

#[derive(Debug)]
pub struct Db { pub conn: Connection }

impl Db {
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

    pub fn connect(path: &Path) -> Result<Self> {
        Self::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
    }

    fn open_with_flags(path: &Path, flags: OpenFlags) -> Result<Self> {
        let conn = Connection::open_with_flags(path, flags)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
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
    pub fn promotion(&self) -> crate::db::repo::promotion::PromotionRepo<'_> {
        crate::db::repo::promotion::PromotionRepo { conn: &self.conn }
    }
    pub fn batches(&self) -> BatchRepo<'_> { BatchRepo { conn: &self.conn } }
    pub fn workflow_results(&self) -> WorkflowResultRepo<'_> {
        WorkflowResultRepo { conn: &self.conn }
    }
    pub fn overview(&self) -> OverviewRepo<'_> { OverviewRepo { conn: &self.conn } }
    pub fn ai_call_logs(&self) -> AiCallLogRepo<'_> {
        AiCallLogRepo { conn: &self.conn }
    }
    pub fn chapter_previews(&self) -> ChapterPreviewRepo<'_> {
        ChapterPreviewRepo { conn: &self.conn }
    }

    pub fn seed_builtin_prompts(&self) -> Result<()> {
        self.prompts().seed_builtin_if_empty()
    }

    pub fn applied_schema_versions(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT version FROM schema_versions ORDER BY LENGTH(version), version",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    #[doc(hidden)]
    pub fn execute_batch(&self, sql: &str) -> rusqlite::Result<()> {
        self.conn.execute_batch(sql)
    }
}

fn run_schemas(conn: &Connection) -> Result<()> {
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
