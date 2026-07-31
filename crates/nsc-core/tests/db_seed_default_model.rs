use nsc_core::db::Db;
use nsc_core::models::{default_from_env, NewModelConfig};

// 这三个 env key 是 default_from_env 必读项,测试中按需设置/清理。
const ENV: [&str; 4] = [
    "NSC_DEFAULT_MODEL_NAME",
    "NSC_DEFAULT_MODEL_BASE_URL",
    "NSC_DEFAULT_MODEL_API_KEY",
    "NSC_DEFAULT_MODEL_MODEL",
];

struct EnvGuard {
    saved: Vec<Option<String>>,
}

impl EnvGuard {
    fn set(kv: &[(&str, &str)]) -> Self {
        let saved: Vec<_> = ENV.iter().map(|k| std::env::var(k).ok()).collect();
        for v in ENV { std::env::remove_var(v); }
        for (k, v) in kv { std::env::set_var(k, v); }
        EnvGuard { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, prev) in ENV.iter().zip(self.saved.drain(..)) {
            match prev {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

#[test]
#[serial_test::serial]
fn empty_env_returns_none() {
    let _g = EnvGuard::set(&[]);
    assert!(default_from_env().is_none());
}

#[test]
#[serial_test::serial]
fn partial_env_returns_none() {
    let _g = EnvGuard::set(&[
        ("NSC_DEFAULT_MODEL_NAME", "env-default"),
        ("NSC_DEFAULT_MODEL_BASE_URL", "https://api.deepseek.com/v1"),
    ]);
    assert!(default_from_env().is_none());
}

#[test]
#[serial_test::serial]
fn full_env_returns_seed() {
    let _g = EnvGuard::set(&[
        ("NSC_DEFAULT_MODEL_NAME", "env-default"),
        ("NSC_DEFAULT_MODEL_BASE_URL", "https://api.deepseek.com/v1"),
        ("NSC_DEFAULT_MODEL_API_KEY", "sk-placeholder"),
        ("NSC_DEFAULT_MODEL_MODEL", "deepseek-chat"),
    ]);
    let s = default_from_env().expect("seed");
    assert_eq!(s.name, "env-default");
    assert_eq!(s.base_url, "https://api.deepseek.com/v1");
    assert_eq!(s.model, "deepseek-chat");
    assert_eq!(s.concurrency, 3); // 默认 3
}

#[test]
#[serial_test::serial]
fn blank_api_key_is_rejected() {
    let _g = EnvGuard::set(&[
        ("NSC_DEFAULT_MODEL_NAME", "env-default"),
        ("NSC_DEFAULT_MODEL_BASE_URL", "https://api.deepseek.com/v1"),
        ("NSC_DEFAULT_MODEL_API_KEY", "   "),
        ("NSC_DEFAULT_MODEL_MODEL", "deepseek-chat"),
    ]);
    assert!(default_from_env().is_none());
}

#[test]
#[serial_test::serial]
fn seed_inserts_when_table_empty() {
    let _g = EnvGuard::set(&[
        ("NSC_DEFAULT_MODEL_NAME", "env-default"),
        ("NSC_DEFAULT_MODEL_BASE_URL", "https://api.deepseek.com/v1"),
        ("NSC_DEFAULT_MODEL_API_KEY", "sk-placeholder"),
        ("NSC_DEFAULT_MODEL_MODEL", "deepseek-chat"),
    ]);
    let db = Db::open_in_memory().unwrap();
    let id = db.seed_default_model_from_env().expect("ok").expect("some");
    assert!(id > 0);
    assert_eq!(db.model_configs().list().unwrap().len(), 1);
    let row = db.model_configs().get(id).unwrap().unwrap();
    assert_eq!(row.name, "env-default");
    assert_eq!(row.api_key, "sk-placeholder");
}

#[test]
#[serial_test::serial]
fn seed_skips_when_user_already_configured() {
    let _g = EnvGuard::set(&[
        ("NSC_DEFAULT_MODEL_NAME", "env-default"),
        ("NSC_DEFAULT_MODEL_BASE_URL", "https://api.deepseek.com/v1"),
        ("NSC_DEFAULT_MODEL_API_KEY", "sk-placeholder"),
        ("NSC_DEFAULT_MODEL_MODEL", "deepseek-chat"),
    ]);
    let db = Db::open_in_memory().unwrap();
    // 用户先建一条
    let user_id = db.model_configs().insert(&NewModelConfig {
        name: "user-model".into(),
        base_url: "https://api.openai.com/v1".into(),
        api_key: "sk-user".into(),
        model: "gpt-4".into(),
        max_tokens: None, temperature: None, concurrency: 3,
    }).unwrap();

    let out = db.seed_default_model_from_env().expect("ok");
    assert!(out.is_none(), "已有用户配置时不应再插 env 种子");

    // 仍是 1 条,且是用户那条
    let list = db.model_configs().list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, user_id);
    assert_eq!(list[0].name, "user-model");
}

#[test]
#[serial_test::serial]
fn seed_no_op_when_env_missing() {
    let _g = EnvGuard::set(&[]);
    let db = Db::open_in_memory().unwrap();
    assert!(db.seed_default_model_from_env().expect("ok").is_none());
    assert_eq!(db.model_configs().list().unwrap().len(), 0);
}
