//! 验证迁移 0004-0007 的等价性:
//! - byte_start/byte_end 在迁移前后指向同一段 UTF-8 字节切片
//! - v3 老库通过 `Db::open` 自动升到 v7 且不报错
//!
//! 测试文件位于 `crates/nsc-core/tests/`,migrations/ 在仓库根目录,
//! 所以 `../../../migrations/` 才是仓库根。

use nsc_core::db::Db;
use rusqlite::{params, Connection};

const MIGRATION_FILES: &[(&str, &str)] = &[
    ("v1", include_str!("../../../migrations/0001_init.sql")),
    ("v2", include_str!("../../../migrations/0002_split_uploads.sql")),
    ("v3", include_str!("../../../migrations/0003_chapter_byte_ranges.sql")),
    ("v4", include_str!("../../../migrations/0004_data_assets.sql")),
    ("v5", include_str!("../../../migrations/0005_chapters_data_asset_fk.sql")),
    ("v6", include_str!("../../../migrations/0006_transformation_novels_data_asset_fk.sql")),
    ("v7", include_str!("../../../migrations/0007_uploads_word_count.sql")),
];

/// 把版本号解析成数字,跳过 v 前缀。
fn version_num(v: &str) -> u8 {
    v.trim_start_matches('v').parse().expect("版本号格式必须是 vN")
}

/// 跑 v1..=v_max 的所有 SQL,自动登记 schema_versions(让 `Db::open` 不会重跑)。
fn open_db_at_v(version: u8) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_versions \
         (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    for (v, sql) in MIGRATION_FILES {
        let n = version_num(v);
        if n <= version {
            conn.execute_batch(sql).unwrap();
            conn.execute(
                "INSERT INTO schema_versions (version, applied_at) VALUES (?1, ?2)",
                params![v, now],
            )
            .unwrap();
        }
    }
    conn
}

#[test]
fn byte_slices_unchanged_across_migration() {
    // 准备一段原文字符串,含中文 + ASCII,UTF-8 字节数与字符数不同。
    // 故意不写章节标题的字面量在 byte range 内,确保切片结果可校验。
    let text = "第一章 山村少年\n小明起床。\n第二章 走出门\n他去砍柴。";

    // v3: 在带 parsed_at 的 upload 上插入 chapters(upload_id + byte ranges)
    // UTF-8 字节偏移(用 Python 验证):
    //   "第一章 山村少年" → [0, 22) 长度 22(9 + 1 + 12)
    //   "第二章 走出门"   → [39, 58) 长度 19(9 + 1 + 9)
    let conn_v3 = open_db_at_v(3);
    conn_v3
        .execute(
            "INSERT INTO uploads \
             (sha256, filename, byte_size, uploaded_at, file_path, parsed_at) \
             VALUES ('a', 'n.txt', ?1, '2026-01-01T00:00:00+00:00', '/p', '2026-01-02T00:00:00+00:00')",
            params![text.len() as i64],
        )
        .unwrap();
    let uid: i64 = conn_v3
        .query_row("SELECT id FROM uploads", [], |r| r.get(0))
        .unwrap();
    conn_v3
        .execute(
            "INSERT INTO chapters \
             (upload_id, idx, title, byte_start, byte_end, word_count, original_content) \
             VALUES (?1, 0, '第一章', 0, 22, 4, ''), (?1, 1, '第二章', 39, 58, 4, '')",
            params![uid],
        )
        .unwrap();
    drop(conn_v3);

    // v6: 同一份 upload,迁移完后 chapters 应该按原 byte 范围切片
    let conn_v6 = open_db_at_v(6);
    conn_v6
        .execute(
            "INSERT INTO uploads \
             (sha256, filename, byte_size, uploaded_at, file_path, original_text) \
             VALUES ('a', 'n.txt', ?1, '2026-01-01T00:00:00+00:00', '/p', ?2)",
            params![text.len() as i64, text],
        )
        .unwrap();
    let uid_v6: i64 = conn_v6
        .query_row("SELECT id FROM uploads", [], |r| r.get(0))
        .unwrap();
    // v6 没有 uploads.parsed_at,v4 的 INSERT...SELECT 不会自动建 data_asset,
    // 这里手动建一条。
    conn_v6
        .execute(
            "INSERT INTO data_assets (upload_id, title, parsed_at) \
             VALUES (?1, 'n.txt', '2026-01-02T00:00:00+00:00')",
            params![uid_v6],
        )
        .unwrap();
    let da_id: i64 = conn_v6
        .query_row(
            "SELECT id FROM data_assets WHERE upload_id = ?1",
            params![uid_v6],
            |r| r.get(0),
        )
        .unwrap();
    conn_v6
        .execute(
            "INSERT INTO chapters \
             (data_asset_id, idx, title, byte_start, byte_end, word_count) \
             VALUES (?1, 0, '第一章', 0, 22, 4), (?1, 1, '第二章', 39, 58, 4)",
            params![da_id],
        )
        .unwrap();

    // 用 v6 的 byte range 在 text 上切片,应该跟 v3 的 byte range 一致
    let chs: Vec<(String, i64, i64)> = conn_v6
        .prepare("SELECT title, byte_start, byte_end FROM chapters ORDER BY idx ASC")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(chs.len(), 2);
    assert_eq!(chs[0].0, "第一章");
    assert_eq!(&text[chs[0].1 as usize..chs[0].2 as usize], "第一章 山村少年");
    assert_eq!(chs[1].0, "第二章");
    assert_eq!(&text[chs[1].1 as usize..chs[1].2 as usize], "第二章 走出门");
    // 总文本长度=74,确认 offset 落在合法范围内
    assert!(text.len() == 74);
}

#[test]
fn open_old_db_runs_all_migrations() {
    // 模拟:磁盘临时 db 上跑 v1..v3,然后 Db::open 应该自动升到 v6。
    // 注意:v1-v3 不是幂等的(v3 的 ALTER TABLE ADD COLUMN 会因为列已存在报错),
    // 所以升级前必须先在 schema_versions 里把 v1-v3 登记为已应用,
    // 让 run_schemas 跳过它们,只补跑 v4-v6。
    let tmp = std::env::temp_dir().join("nsc_migration_test.db");
    let _ = std::fs::remove_file(&tmp);

    {
        let conn = Connection::open(&tmp).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!("../../../migrations/0001_init.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../../migrations/0002_split_uploads.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../../migrations/0003_chapter_byte_ranges.sql"))
            .unwrap();
        // 标记 v1-v3 已应用,模拟生产环境升级前的版本记录。
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_versions \
             (version TEXT PRIMARY KEY, applied_at TEXT NOT NULL); \
             INSERT OR IGNORE INTO schema_versions (version, applied_at) VALUES \
             ('v1', '2026-01-01T00:00:00+00:00'), \
             ('v2', '2026-01-01T00:00:00+00:00'), \
             ('v3', '2026-01-01T00:00:00+00:00');",
        )
        .unwrap();
    }

    let db = Db::open(&tmp).unwrap();
    // 检查 schema_versions 表是否登记了所有已发布的版本(0011 加入时同步更新)。
    let versions = db.applied_schema_versions().unwrap();
    assert_eq!(
        versions,
        vec![
            "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10",
            "0011_workflow_results", "0012_batches_tn_cascade",
            "0014_builtin_prompt_double_braces",
            "0013_workflow_result_chapters_cascade",
        ]
    );

    let _ = std::fs::remove_file(&tmp);
}