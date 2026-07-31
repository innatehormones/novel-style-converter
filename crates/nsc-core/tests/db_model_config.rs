use nsc_core::db::Db;
use nsc_core::models::NewModelConfig;

#[test]
fn crud() {
    let db = Db::open_in_memory().unwrap();
    let id = db.model_configs().insert(&NewModelConfig {
        name: "gpt-4".into(),
        base_url: "https://api.openai.com/v1".into(),
        api_key: "sk-test".into(),
        model: "gpt-4".into(),
        max_tokens: Some(4096),
        temperature: Some(0.7),
        concurrency: 3,
    }).unwrap();

    let list = db.model_configs().list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, id);

    let mut mc = db.model_configs().get(id).unwrap().unwrap();
    mc.temperature = Some(0.3);
    db.model_configs().update(&mc).unwrap();
    assert_eq!(
        db.model_configs().get(id).unwrap().unwrap().temperature,
        Some(0.3)
    );

    db.model_configs().delete(id).unwrap();
    assert!(db.model_configs().get(id).unwrap().is_none());
}
