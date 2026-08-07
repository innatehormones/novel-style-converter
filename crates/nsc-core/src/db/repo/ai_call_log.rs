use rusqlite::{params, params_from_iter, Connection, Row};

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::models::{AiCallBusiness, AiCallLog, AiCallLogFilter, AiCallStatus, NewAiCallLog};

/// ai_call_logs 表 repo —— 写时直接用 NewAiCallLog;读时返回 AiCallLog。
/// list(filter) 按时间倒序,带 business / model_config_id / status 三个可选过滤 + limit。
pub struct AiCallLogRepo<'a> { pub(crate) conn: &'a Connection }

/// prompt / response 预览上限 —— 10KB。
/// 截断的字节数够展示 1-2 屏内容,排查时够定位"是模型没听 prompt 还是参数填错了";
/// 完整内容由调用方自己保留(transform 路径: transformation_chapters.result_content;test_model 路径:不存在全文)。
pub const PREVIEW_BYTES: usize = 10 * 1024;

/// 截取前 10KB 预览 + 记总字符数。
/// 公共工具:recorder / 单元测试都复用同一份截断逻辑,避免哪边少截了 1 字符的隐性不一致。
/// 返回 (preview_opt, total_chars) —— 字符串为空时存 NULL(size=0),不写空串。
pub fn truncate_preview(s: &str) -> (Option<String>, i64) {
    if s.is_empty() {
        return (None, 0);
    }
    let total = s.chars().count() as i64;
    if s.len() <= PREVIEW_BYTES {
        (Some(s.to_string()), total)
    } else {
        // 字节截断要保证落在 char 边界,不能简单地 &s[..N]
        let mut end = PREVIEW_BYTES;
        for (i, (b, _)) in s.char_indices().enumerate() {
            if i == PREVIEW_BYTES {
                end = b;
                break;
            }
        }
        (Some(s[..end].to_string()), total)
    }
}

impl<'a> AiCallLogRepo<'a> {
    /// 插一行。created_at = 当前 UTC,id 自增。
    /// insert 失败时直接 Err —— recorder / 命令层不吞错,排查链路要保持。
    pub fn insert(&self, n: &NewAiCallLog) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        let business = match n.business {
            AiCallBusiness::TransformChapter => "transform_chapter",
            AiCallBusiness::TestModel => "test_model",
        };
        let status = match n.status {
            AiCallStatus::Success => "success",
            AiCallStatus::Failed => "failed",
        };
        // 混合 Option<T> 和 T 的 bind —— Box<dyn ToSql> 让 rusqlite 接受异构参数。
        let binds: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(now),
            Box::new(business),
            Box::new(n.context_type.clone()),
            Box::new(n.context_id),
            Box::new(n.model_config_id),
            Box::new(n.model_name.clone()),
            Box::new(n.base_url.clone()),
            Box::new(n.temperature),
            Box::new(n.max_tokens),
            Box::new(n.system_preview.clone()),
            Box::new(n.user_preview.clone()),
            Box::new(n.system_size),
            Box::new(n.user_size),
            Box::new(n.estimated_tokens_in),
            Box::new(n.actual_tokens_in),
            Box::new(n.actual_tokens_out),
            Box::new(status),
            Box::new(n.response_preview.clone()),
            Box::new(n.response_size),
            Box::new(n.latency_ms),
            Box::new(n.error.clone()),
        ];
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        self.conn.execute(
            "INSERT INTO ai_call_logs (
                created_at, business, context_type, context_id,
                model_config_id, model_name, base_url,
                temperature, max_tokens,
                system_preview, user_preview, system_size, user_size,
                estimated_tokens_in, actual_tokens_in, actual_tokens_out,
                status, response_preview, response_size, latency_ms, error
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
            rusqlite::params_from_iter(bind_refs),
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 按 AiCallLogFilter 过滤 + 时间倒序 + 限制行数。
    /// 任意过滤项为 None 时不过滤。limit 缺省 200(UI 列表初次拉 + 翻页都用这个);
    /// 上限 1000,防止 UI 误传 1e9 把 DB 拉死。
    pub fn list(&self, filter: &AiCallLogFilter) -> Result<Vec<AiCallLog>> {
        let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
        let mut sql = String::from(
            "SELECT id, created_at, business, context_type, context_id,
                    model_config_id, model_name, base_url,
                    temperature, max_tokens,
                    system_preview, user_preview, system_size, user_size,
                    estimated_tokens_in, actual_tokens_in, actual_tokens_out,
                    status, response_preview, response_size, latency_ms, error
               FROM ai_call_logs WHERE 1=1",
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(b) = filter.business {
            sql.push_str(" AND business = ?");
            binds.push(Box::new(match b {
                AiCallBusiness::TransformChapter => "transform_chapter",
                AiCallBusiness::TestModel => "test_model",
            }));
        }
        if let Some(mc) = filter.model_config_id {
            sql.push_str(" AND model_config_id = ?");
            binds.push(Box::new(mc));
        }
        if let Some(s) = filter.status {
            sql.push_str(" AND status = ?");
            binds.push(Box::new(match s {
                AiCallStatus::Success => "success",
                AiCallStatus::Failed => "failed",
            }));
        }
        sql.push_str(" ORDER BY created_at DESC, id DESC LIMIT ");
        sql.push_str(&limit.to_string());

        let mut stmt = self.conn.prepare(&sql)?;
        let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(params_from_iter(bind_refs), |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// 按 id 查单行 —— 不分 status,失败 / 成功都返回。
    pub fn get(&self, id: i64) -> Result<Option<AiCallLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, business, context_type, context_id,
                    model_config_id, model_name, base_url,
                    temperature, max_tokens,
                    system_preview, user_preview, system_size, user_size,
                    estimated_tokens_in, actual_tokens_in, actual_tokens_out,
                    status, response_preview, response_size, latency_ms, error
               FROM ai_call_logs WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// 清空全部日志 —— UI 看板"清空"按钮用。
    /// 不做软删,直接 DELETE FROM —— 表结构允许,UI 显式按钮触发,没有"静默丢数据"风险。
    /// 返回删除行数(供 UI toast 用)。
    pub fn clear(&self) -> Result<usize> {
        let n = self.conn.execute("DELETE FROM ai_call_logs", [])?;
        Ok(n)
    }

    /// 按 context 软引用反查 —— 从 transformation_chapter 找历史 AI 调用。
    /// 软引用,无 FK,所以业务对象删了日志仍能查到。
    pub fn list_by_context(&self, context_type: &str, context_id: i64) -> Result<Vec<AiCallLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, business, context_type, context_id,
                    model_config_id, model_name, base_url,
                    temperature, max_tokens,
                    system_preview, user_preview, system_size, user_size,
                    estimated_tokens_in, actual_tokens_in, actual_tokens_out,
                    status, response_preview, response_size, latency_ms, error
               FROM ai_call_logs WHERE context_type = ?1 AND context_id = ?2
               ORDER BY created_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![context_type, context_id], |row| from_row(row))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn from_row(row: &Row) -> rusqlite::Result<AiCallLog> {
    let created_at_s: String = row.get(1)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
    let business_s: String = row.get(2)?;
    let business = match business_s.as_str() {
        "transform_chapter" => AiCallBusiness::TransformChapter,
        _ => AiCallBusiness::TestModel,
    };
    let status_s: String = row.get(17)?;
    let status = match status_s.as_str() {
        "success" => AiCallStatus::Success,
        _ => AiCallStatus::Failed,
    };
    Ok(AiCallLog {
        id: row.get(0)?,
        created_at,
        business,
        context_type: row.get(3)?,
        context_id: row.get(4)?,
        model_config_id: row.get(5)?,
        model_name: row.get(6)?,
        base_url: row.get(7)?,
        temperature: row.get(8)?,
        max_tokens: row.get(9)?,
        system_preview: row.get(10)?,
        user_preview: row.get(11)?,
        system_size: row.get(12)?,
        user_size: row.get(13)?,
        estimated_tokens_in: row.get(14)?,
        actual_tokens_in: row.get(15)?,
        actual_tokens_out: row.get(16)?,
        status,
        response_preview: row.get(18)?,
        response_size: row.get(19)?,
        latency_ms: row.get(20)?,
        error: row.get(21)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_log(b: AiCallBusiness) -> NewAiCallLog {
        NewAiCallLog {
            business: b,
            context_type: Some("transformation_chapter".into()),
            context_id: Some(42),
            model_config_id: Some(1),
            model_name: "gpt-4o-mini".into(),
            base_url: "https://api.example.com/v1".into(),
            temperature: Some(0.7),
            max_tokens: Some(2048),
            system_preview: Some("you are a translator".into()),
            user_preview: Some("hello world".into()),
            system_size: 22,
            user_size: 11,
            estimated_tokens_in: Some(6),
            actual_tokens_in: Some(8),
            actual_tokens_out: Some(20),
            status: AiCallStatus::Success,
            response_preview: Some("hi".into()),
            response_size: 2,
            latency_ms: 1234,
            error: None,
        }
    }

    #[test]
    fn insert_and_list_roundtrip() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let repo = db.ai_call_logs();
        let id = repo.insert(&new_log(AiCallBusiness::TransformChapter)).unwrap();
        assert!(id > 0);
        let logs = repo.list(&AiCallLogFilter::default()).unwrap();
        assert_eq!(logs.len(), 1);
        let got = &logs[0];
        assert_eq!(got.id, id);
        assert_eq!(got.business, AiCallBusiness::TransformChapter);
        assert_eq!(got.model_name, "gpt-4o-mini");
        assert_eq!(got.status, AiCallStatus::Success);
        assert_eq!(got.actual_tokens_out, Some(20));
    }

    #[test]
    fn list_filter_by_business_and_status() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let repo = db.ai_call_logs();
        repo.insert(&new_log(AiCallBusiness::TransformChapter)).unwrap();
        repo.insert(&new_log(AiCallBusiness::TestModel)).unwrap();
        let mut bad = new_log(AiCallBusiness::TransformChapter);
        bad.status = AiCallStatus::Failed;
        bad.error = Some("oops".into());
        repo.insert(&bad).unwrap();
        let f = AiCallLogFilter { business: Some(AiCallBusiness::TestModel), ..Default::default() };
        assert_eq!(repo.list(&f).unwrap().len(), 1);
        let f = AiCallLogFilter { status: Some(AiCallStatus::Failed), ..Default::default() };
        assert_eq!(repo.list(&f).unwrap().len(), 1);
        let f = AiCallLogFilter { status: Some(AiCallStatus::Success), ..Default::default() };
        assert_eq!(repo.list(&f).unwrap().len(), 2);
    }

    #[test]
    fn clear_empties_table() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let repo = db.ai_call_logs();
        repo.insert(&new_log(AiCallBusiness::TestModel)).unwrap();
        repo.insert(&new_log(AiCallBusiness::TestModel)).unwrap();
        let n = repo.clear().unwrap();
        assert_eq!(n, 2);
        assert!(repo.list(&AiCallLogFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn truncate_preview_short_text_unchanged() {
        let (p, n) = truncate_preview("hello");
        assert_eq!(p.as_deref(), Some("hello"));
        assert_eq!(n, 5);
    }

    #[test]
    fn truncate_preview_empty_returns_none() {
        let (p, n) = truncate_preview("");
        assert!(p.is_none());
        assert_eq!(n, 0);
    }

    #[test]
    fn truncate_preview_caps_at_10kb_char_boundary() {
        // 100 个中文字 = 300 字节,远小于 10KB —— 应原样
        let s = "测".repeat(100);
        let (p, n) = truncate_preview(&s);
        assert_eq!(p.as_deref().unwrap().chars().count(), 100);
        assert_eq!(n, 100);

        // 20000 个中文字 = 60000 字节,远超 10KB —— 应截到 10240 字符(每个汉字 3 字节)
        let s = "测".repeat(20_000);
        let (p, n) = truncate_preview(&s);
        assert_eq!(p.as_ref().unwrap().chars().count(), PREVIEW_BYTES);
        assert_eq!(n, 20_000);
        // 截断后仍是合法 UTF-8(不会在字符中间切)
        assert!(p.unwrap().ends_with("测"));
    }
}