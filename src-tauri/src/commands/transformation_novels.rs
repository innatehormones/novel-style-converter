use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use nsc_core::db::Db;
use nsc_core::models::{NewTransformationNovel, PromptKind, TransformationNovel};

/// 列表返回的 tn 摘要。`default_*` 三字段为 None 表示用户未设定默认配置,
/// 新建时就用 `None`(兼容旧 tn);前端 TnDialog 用这三字段做表单回显。
#[derive(Debug, Serialize)]
pub struct TransformationNovelSummary {
    pub id: i64,
    pub data_asset_id: i64,
    pub title: String,
    pub created_at: String,
    pub chapters_count: i64,
    pub default_model_config_id: Option<i64>,
    pub default_prompt_id: Option<i64>,
    pub default_mode: Option<PromptKind>,
}

/// 创建 transformation_novel 的入参。inner DTO 字段保持 snake_case
/// (与 Tauri 的 camelCase outer 自动翻译区分开);三个默认字段都允许缺省
/// 或 `null` —— 用于旧 tn 兼容以及前端 dialog 让用户稍后再补。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateTransformationNovelPayload {
    pub data_asset_id: i64,
    pub title: String,
    #[serde(default)]
    pub default_model_config_id: Option<i64>,
    #[serde(default)]
    pub default_prompt_id: Option<i64>,
    /// `null` 与字段缺省等价,都映射为 `None`。
    #[serde(default)]
    pub default_mode: Option<PromptKind>,
}

/// 更新 transformation_novel 的入参。注意三个默认字段来自 payload 而非沿用 `cur`,
/// 这样前端可显式把默认配置改成 `null`(清空存量默认值)。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTransformationNovelPayload {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub default_model_config_id: Option<i64>,
    #[serde(default)]
    pub default_prompt_id: Option<i64>,
    #[serde(default)]
    pub default_mode: Option<PromptKind>,
}

fn to_summary(db: &Db, n: &TransformationNovel) -> TransformationNovelSummary {
    let chapters_count = db
        .chapters()
        .list_by_data_asset(n.data_asset_id)
        .map(|v| v.len() as i64)
        .unwrap_or(0);
    TransformationNovelSummary {
        id: n.id,
        data_asset_id: n.data_asset_id,
        title: n.title.clone(),
        created_at: n.created_at.to_rfc3339(),
        chapters_count,
        default_model_config_id: n.default_model_config_id,
        default_prompt_id: n.default_prompt_id,
        default_mode: n.default_mode,
    }
}

#[tauri::command]
pub fn list_transformation_novels(
    db: State<'_, Arc<Mutex<Db>>>,
    data_asset_id: Option<i64>,
) -> Result<Vec<TransformationNovelSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let all = match data_asset_id {
        Some(da_id) => db
            .transformation_novels()
            .list_by_data_asset(da_id)
            .map_err(|e| e.to_string())?,
        None => db
            .transformation_novels()
            .list()
            .map_err(|e| e.to_string())?,
    };
    Ok(all.iter().map(|n| to_summary(&db, n)).collect())
}

/// 新建 transformation_novel。先校验 `data_asset_id` 存在 + title 非空;
/// 同 data_asset 允许多本 transformation_novel(每本独立 prompt / model / 上下文)。
/// 返回新 `transformation_novel.id`。
#[tauri::command]
pub fn create_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: CreateTransformationNovelPayload,
) -> Result<i64, String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let _da = db
        .data_assets()
        .get(payload.data_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {} 不存在", payload.data_asset_id))?;
    db.transformation_novels()
        .insert(&NewTransformationNovel {
            data_asset_id: payload.data_asset_id,
            title: title.to_string(),
            default_model_config_id: payload.default_model_config_id,
            default_prompt_id: payload.default_prompt_id,
            default_mode: payload.default_mode,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UpdateTransformationNovelPayload,
) -> Result<(), String> {
    let title = payload.title.trim();
    if title.is_empty() {
        return Err("标题不能为空".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let cur = db
        .transformation_novels()
        .get(payload.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("transformation_novel {} 不存在", payload.id))?;
    let next = TransformationNovel {
        id: cur.id,
        data_asset_id: cur.data_asset_id,
        title: title.to_string(),
        created_at: cur.created_at,
        default_model_config_id: payload.default_model_config_id,
        default_prompt_id: payload.default_prompt_id,
        default_mode: payload.default_mode,
    };
    db.transformation_novels().update(&next).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_transformation_novel(
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.transformation_novels().delete(id).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    //! T4 阶段:验证 `CreateTransformationNovelPayload` / `UpdateTransformationNovelPayload`
    //! 的 serde 形状 —— inner DTO 必须保持 snake_case,三个默认字段允许缺省,
    //! 且 `default_mode` 字符串 ("compress" | "style") 能反序列化为 `PromptKind`。
    //!
    //! 这里只覆盖反序列化形状;命令本身的 DB 写入逻辑由
    //! `crates/nsc-core/tests/db_tn_default_columns.rs` 的端到端 roundtrip
    //! 间接覆盖(已验证 NewTransformationNovel 三字段持久化路径)。
    use super::{CreateTransformationNovelPayload, UpdateTransformationNovelPayload};
    use nsc_core::models::PromptKind;
    use serde_json::json;

    #[test]
    fn create_payload_deserializes_full_snake_case_payload() {
        let raw = json!({
            "data_asset_id": 42,
            "title": "  斗破_热血版  ",
            "default_model_config_id": 7,
            "default_prompt_id": 3,
            "default_mode": "style",
        });
        let p: CreateTransformationNovelPayload = serde_json::from_value(raw).expect("serde");
        assert_eq!(p.data_asset_id, 42);
        assert_eq!(p.title, "  斗破_热血版  "); // trim 在命令里做,payload 原样透传
        assert_eq!(p.default_model_config_id, Some(7));
        assert_eq!(p.default_prompt_id, Some(3));
        assert_eq!(p.default_mode, Some(PromptKind::Style));
    }

    #[test]
    fn create_payload_optional_default_fields_can_be_omitted() {
        // 旧 tn 兼容:前端老调用方 / 迁移期 payload 不带三默认字段也能解析。
        let raw = json!({ "data_asset_id": 1, "title": "legacy" });
        let p: CreateTransformationNovelPayload = serde_json::from_value(raw).expect("serde");
        assert_eq!(p.data_asset_id, 1);
        assert_eq!(p.title, "legacy");
        assert_eq!(p.default_model_config_id, None);
        assert_eq!(p.default_prompt_id, None);
        assert_eq!(p.default_mode, None);
    }

    #[test]
    fn create_payload_rejects_camel_case_outer_keys() {
        // inner DTO 必须 snake_case,前端 camelCase key 必须失败 —— Tauri 的 camelCase
        // 自动翻译只作用于 outer invoke args,不会改 payload 内容。
        let raw = json!({
            "dataAssetId": 1,
            "title": "x",
            "defaultModelConfigId": 7,
            "defaultPromptId": 3,
            "defaultMode": "style",
        });
        let r: Result<CreateTransformationNovelPayload, _> = serde_json::from_value(raw);
        assert!(r.is_err(), "camelCase keys must not deserialize into snake_case DTO");
    }

    #[test]
    fn update_payload_round_trips_snake_case_fields() {
        let raw = json!({
            "id": 99,
            "title": "new title",
            "default_model_config_id": 11,
            "default_prompt_id": null,
            "default_mode": "compress",
        });
        let p: UpdateTransformationNovelPayload = serde_json::from_value(raw).expect("serde");
        assert_eq!(p.id, 99);
        assert_eq!(p.title, "new title");
        assert_eq!(p.default_model_config_id, Some(11));
        assert_eq!(p.default_prompt_id, None); // null -> None
        assert_eq!(p.default_mode, Some(PromptKind::Compress));
    }

    #[test]
    fn update_payload_all_optional_defaults_may_be_null() {
        let raw = json!({
            "id": 5,
            "title": "t",
            "default_model_config_id": null,
            "default_prompt_id": null,
            "default_mode": null,
        });
        let p: UpdateTransformationNovelPayload = serde_json::from_value(raw).expect("serde");
        assert_eq!(p.default_model_config_id, None);
        assert_eq!(p.default_prompt_id, None);
        assert_eq!(p.default_mode, None);
    }

    #[test]
    fn default_mode_string_must_match_snake_case_variant() {
        // "Compress" / "STYLE" 等任何非 snake_case 字面量都应被 serde 拒绝。
        for bad in ["Compress", "STYLE", "Style", "other"] {
            let raw = json!({
                "data_asset_id": 1,
                "title": "x",
                "default_model_config_id": null,
                "default_prompt_id": null,
                "default_mode": bad,
            });
            let r: Result<CreateTransformationNovelPayload, _> = serde_json::from_value(raw);
            assert!(
                r.is_err(),
                "default_mode={bad:?} must fail to deserialize as PromptKind snake_case"
            );
        }
    }

    #[test]
    fn summary_serializes_default_fields_in_snake_case() {
        // 前端 TransformationNovelSummary 期望三个 default_* 字段,
        // 这里直接断言序列化输出包含它们,且 default_mode 用 snake_case 字面量。
        let s = super::TransformationNovelSummary {
            id: 1,
            data_asset_id: 2,
            title: "t".into(),
            created_at: "1970-01-01T00:00:00Z".into(),
            chapters_count: 5,
            default_model_config_id: Some(7),
            default_prompt_id: None,
            default_mode: Some(PromptKind::Style),
        };
        let v: serde_json::Value = serde_json::to_value(&s).expect("serialize");
        assert_eq!(v["default_model_config_id"], serde_json::json!(7));
        assert_eq!(v["default_prompt_id"], serde_json::json!(null));
        assert_eq!(v["default_mode"], serde_json::json!("style"));
    }
}