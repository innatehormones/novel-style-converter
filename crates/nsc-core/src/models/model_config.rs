use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub concurrency: i32,
}

#[derive(Debug, Clone)]
pub struct NewModelConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
    pub concurrency: i32,
}

/// 从进程环境变量读兜底模型配置。任一字段缺失或空白返回 `None`,由调用方决定
/// 是否作为"用户尚未配置任何模型"的兜底。
///
/// 环境变量:
/// - `NSC_DEFAULT_MODEL_NAME`      必填(显示名,如 "env-default")
/// - `NSC_DEFAULT_MODEL_BASE_URL`  必填(OpenAI 兼容 endpoint)
/// - `NSC_DEFAULT_MODEL_API_KEY`   必填(可填占位符 `sk-placeholder` 后续在 UI 编辑)
/// - `NSC_DEFAULT_MODEL_MODEL`     必填(如 "deepseek-chat")
/// - `NSC_DEFAULT_MODEL_MAX_TOKENS`     可选
/// - `NSC_DEFAULT_MODEL_TEMPERATURE`    可选
/// - `NSC_DEFAULT_MODEL_CONCURRENCY`    可选(默认 3)
pub fn default_from_env() -> Option<NewModelConfig> {
    fn opt<T: std::str::FromStr>(k: &str) -> Option<T> {
        std::env::var(k).ok().and_then(|v| v.parse().ok())
    }
    fn req(k: &str) -> Option<String> {
        std::env::var(k).ok().filter(|v| !v.trim().is_empty())
    }

    let name = req("NSC_DEFAULT_MODEL_NAME")?;
    let base_url = req("NSC_DEFAULT_MODEL_BASE_URL")?;
    let api_key = req("NSC_DEFAULT_MODEL_API_KEY")?;
    let model = req("NSC_DEFAULT_MODEL_MODEL")?;

    Some(NewModelConfig {
        name,
        base_url,
        api_key,
        model,
        max_tokens: opt("NSC_DEFAULT_MODEL_MAX_TOKENS"),
        temperature: opt("NSC_DEFAULT_MODEL_TEMPERATURE"),
        concurrency: opt("NSC_DEFAULT_MODEL_CONCURRENCY").unwrap_or(3),
    })
}
