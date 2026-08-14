//! Startup cleanup (one-shot, gated by marker table).
//!
//! æµè¯é¶æ®µåè®¸ç ´åæ§ schema æ¹å¨,æ¹å¨åæ®ççæ§æ°æ®å¯è½è·æ°çº¦æå²çªã
//! è¿ç§æ¸çç¨ `cleanup_markers` è¡¨è®°å + æ¶é´æ³,åªå¨ç¬¬ä¸æ¬¡å¯å¨æ¶è·,
//! è·å®å marker,ä»¥åä¸åæ§è¡ã
//!
//! æ°å¢æ¸çé¡¹:å¨ `CLEANUPS` éå ä¸è¡ ãã å(è®°å½åæ°åæ¶é´æ³)ãSQL å¤æ¡ statements ç¨ `;` åéã
//! æµè¯æ¶æ¸åºåæ³éè·,ç´æ¥ `DELETE FROM cleanup_markers`ã

use rusqlite::{params, Connection};

use crate::error::Result;

/// å½åæ¸çé¡¹åè¡¨ãæ°å¢é¡¹ç®æ¶åªé append å°è¿éã
/// åå­æ¹ä¸ä¸ª = éæ°ææ(å ä¸º marker æªå°è¾¾)ã
const CLEANUPS: &[(&str, &str)] = &[
    (
        "2026_08_14_clear_test_data",
        // ä¿ç uploads / prompts / model_configs;å¶ä½è¡¨æ¸ç©ºè®©ç¨æ·éæ°èµ°ä¸éæµç¨ã
        // é¡ºåº:tc â workflow_results â batches â data_assets â ai_call_logsã
        // tc.batch_id æ¯ NO ACTION ä¸ä¼ cascade,å¾åå ;
        // data_assets CASCADE æ¸ chaptersã
        "DELETE FROM transformation_chapters;DELETE FROM workflow_result_chapters;DELETE FROM workflow_results;DELETE FROM batches;DELETE FROM transformation_novels;DELETE FROM data_assets;DELETE FROM ai_call_logs;",
    ),
];

pub fn run(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cleanup_markers (
            name TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL,
            rowcount INTEGER NOT NULL DEFAULT 0
        )",
    )?;

    for (name, sql) in CLEANUPS {
        let applied: bool = conn
            .query_row(
                "SELECT 1 FROM cleanup_markers WHERE name = ?1",
                [name],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if applied {
            continue;
        }

        let tx = conn.unchecked_transaction()?;
        let rowcount: usize = {
            let mut count = 0usize;
            for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                count += tx.execute(stmt, [])?;
            }
            count
        };
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO cleanup_markers (name, applied_at, rowcount) VALUES (?1, ?2, ?3)",
            params![name, now, rowcount as i64],
        )?;
        tx.commit()?;
        eprintln!("[startup_cleanup] applied '{}' ({} rows)", name, rowcount);
    }
    Ok(())
}
