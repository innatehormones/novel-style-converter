use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::transformation::TransformMode;

/// State 1: 一次上传 = 一份原始 .txt 文件。sha256 去重。
/// 章节结构不在此处,章节切片在 data_assets(已拆分)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Upload {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub uploaded_at: DateTime<Utc>,
    pub file_path: String,
    /// 原文整篇。data_assets 的章节切片通过 byte_start/byte_end 在此坐标系定位。
    pub original_text: String,
    /// 字数(zh-aware:汉字 + 字母 + 数字)。upload_file() 时一次性算好。
    /// UI 列表展示用,避免每次 list 都对原文做字符扫描。
    pub word_count: i64,
}

#[derive(Debug, Clone)]
pub struct NewUpload {
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub file_path: String,
    pub original_text: String,
    pub word_count: i64,
}

/// 转换小说工作台条目:引用某个 data_asset(已锁定的 State 2 行)。
/// 同一 data_asset 可被多本 transformation_novel 引用(fan-out)。
/// 创建 transformation_novel 时即触发 data_assets.locked_at = now()。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformationNovel {
    pub id: i64,
    pub data_asset_id: i64,
    pub title: String,
    pub created_at: DateTime<Utc>,
    /// 默认模型配置 id。NULL 兼容存量旧 tn（无默认配置）。
    pub default_model_config_id: Option<i64>,
    /// 默认 prompt id。
    pub default_prompt_id: Option<i64>,
    /// 默认转换模式 ('compress' | 'style')。
    pub default_mode: Option<TransformMode>,
}

#[derive(Debug, Clone)]
pub struct NewTransformationNovel {
    pub data_asset_id: i64,
    pub title: String,
    pub default_model_config_id: Option<i64>,
    pub default_prompt_id: Option<i64>,
    pub default_mode: Option<TransformMode>,
}