//! Migration 0011:workflow_results / workflow_result_chapters 结果表,
//! 以及把存量 batches 回填到结果集、把陈旧 batch 状态归一为 stopped。
//!
//! 测试路径:在裸内存库上跑 v1..v10,塞存量 batch/tc 数据,然后单独跑 0011
//! 的 SQL,断言回填与状态归一都生效。等价于生产中老库从 v10 升 v11。

use rusqlite::{params, Connection};

const V1_V10: &[(&str, &str)] = &[
    ("v1", include_str!("../../../migrations/0001_init.sql")),
    ("v2", include_str!("../../../migrations/0002_split_uploads.sql")),
    ("v3", include_str!("../../../migrations/0003_chapter_byte_ranges.sql")),
    ("v4", include_str!("../../../migrations/0004_data_assets.sql")),
    ("v5", include_str!("../../../migrations/0005_chapters_data_asset_fk.sql")),
    ("v6", include_str!("../../../migrations/0006_transformation_novels_data_asset_fk.sql")),
    ("v7", include_str!("../../../migrations/0007_uploads_word_count.sql")),
    ("v8", include_str!("../../../migrations/0008_tn_default_columns.sql")),
    ("v9", include_str!("../../../migrations/0009_batches.sql")),
    ("v10", include_str!("../../../migrations/0010_tc_batch_columns.sql")),
];

const V11: &str = include_str!("../../../migrations/0011_workflow_results.sql");

fn open_at_v10() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions \
         (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    ).unwrap();
    let now = "2026-08-04T00:00:00+00:00";
    for (version, sql) in V1_V10 {
        conn.execute_batch(sql).unwrap();
        conn.execute(
            "INSERT INTO schema_versions (version, applied_at) VALUES (?1, ?2)",
            params![version, now],
        ).unwrap();
    }
    conn
}

#[test]
fn migration_0011_creates_workflow_results_and_seeds_for_existing_batches() {
    let conn = open_at_v10();
    let now = "2026-08-04T00:00:00+00:00";

    // 准备 FK 链:upload → data_asset → transformation_novels/chapters → batches/tc。
    conn.execute(
        "INSERT INTO uploads \
         (sha256, filename, byte_size, uploaded_at, file_path, original_text) \
         VALUES ('h', 'n.txt', 10, ?1, '/p', '')",
        params![now],
    ).unwrap();
    let upload_id: i64 = conn.query_row("SELECT id FROM uploads", [], |r| r.get(0)).unwrap();
    conn.execute(
        "INSERT INTO data_assets (upload_id, title, parsed_at) VALUES (?1, 'n.txt', ?2)",
        params![upload_id, now],
    ).unwrap();
    let da_id: i64 = conn.query_row("SELECT id FROM data_assets", [], |r| r.get(0)).unwrap();

    // v10 阶段存量数据:一个 running batch + 一条 done task。
    conn.execute(
        "INSERT INTO transformation_novels \
         (title, data_asset_id, default_mode, created_at) \
         VALUES ('tn', ?1, 'compress', ?2)",
        params![da_id, now],
    ).unwrap();
    conn.execute(
        "INSERT INTO chapters \
         (data_asset_id, idx, title, byte_start, byte_end, word_count) \
         VALUES (?1, 1, 'c1', 0, 10, 10)",
        params![da_id],
    ).unwrap();
    let tn_id: i64 = conn.query_row("SELECT id FROM transformation_novels", [], |r| r.get(0)).unwrap();
    let chapter_id: i64 = conn.query_row("SELECT id FROM chapters", [], |r| r.get(0)).unwrap();
    conn.execute(
        "INSERT INTO batches \
         (transformation_novel_id, label, on_failure_policy, status, created_at, started_at) \
         VALUES (?1, NULL, 'pause_and_review', 'running', ?2, ?2)",
        params![tn_id, now],
    ).unwrap();
    let batch_id: i64 = conn.query_row("SELECT id FROM batches", [], |r| r.get(0)).unwrap();
    conn.execute(
        "INSERT INTO transformation_chapters \
         (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
          ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
          batch_id, style_ref_chapter_id, status, result_content, started_at, completed_at) \
         VALUES (?1, ?2, 'compress', 1, 1, 0, 0, 0, ?3, NULL, 'done', 'r1', ?4, ?4)",
        params![tn_id, chapter_id, batch_id, now],
    ).unwrap();

    // 升 v11
    conn.execute_batch(V11).unwrap();
    conn.execute(
        "INSERT INTO schema_versions (version, applied_at) VALUES ('0011_workflow_results', ?1)",
        params![now],
    ).unwrap();

    // 断言结果集已建且已回填
    let result_id: i64 = conn.query_row(
        "SELECT id FROM workflow_results WHERE batch_id = ?1",
        params![batch_id], |r| r.get(0),
    ).expect("结果集应已建立");
    let content: Option<String> = conn.query_row(
        "SELECT content FROM workflow_result_chapters \
         WHERE workflow_result_id = ?1 AND chapter_id = ?2",
        params![result_id, chapter_id], |r| r.get(0),
    ).unwrap();
    assert_eq!(content.as_deref(), Some("r1"));

    // 断言 batch 状态归一为 stopped
    let status: String = conn.query_row(
        "SELECT status FROM batches WHERE id = ?1",
        params![batch_id], |r| r.get(0),
    ).unwrap();
    assert_eq!(status, "stopped");

    // 断言 schema_versions 已登记 0011(以便 run_schemas 不再跑它)。
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM schema_versions WHERE version = '0011_workflow_results'",
        [], |r| r.get(0),
    ).unwrap();
    assert_eq!(n, 1);
}
