use nsc_core::db::Db;
use nsc_core::models::{
    NewDataAsset, NewTransformationNovel, NewUpload, TransformMode, TransformationNovel,
};

#[test]
fn tn_with_default_columns_roundtrip() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x".into(),
        original_text: "正文".into(),
        word_count: 0,
    }).unwrap();
    db.seed_builtin_prompts().unwrap();
    let model_id = db.model_configs().insert(&nsc_core::models::NewModelConfig {
        name: "m".into(),
        base_url: "http://x".into(),
        api_key: "k".into(),
        model: "g".into(),
        max_tokens: None,
        temperature: None,
        concurrency: 1,
    }).unwrap();
    let prompt_id = db.prompts().list().unwrap()[0].id;
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();

    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "tn".into(),
        default_model_config_id: Some(model_id),
        default_prompt_id: Some(prompt_id),
        default_mode: Some(TransformMode::Style),
    }).unwrap();

    let tn: TransformationNovel = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert_eq!(tn.default_model_config_id, Some(model_id));
    assert_eq!(tn.default_prompt_id, Some(prompt_id));
    assert_eq!(tn.default_mode, Some(TransformMode::Style));

    let mut tn2 = tn.clone();
    tn2.default_mode = Some(TransformMode::Compress);
    db.transformation_novels().update(&tn2).unwrap();
    let tn3 = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert_eq!(tn3.default_mode, Some(TransformMode::Compress));
}

#[test]
fn tn_default_columns_optional() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x".into(),
        original_text: "正文".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "legacy".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    let tn = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert!(tn.default_model_config_id.is_none());
    assert!(tn.default_prompt_id.is_none());
    assert!(tn.default_mode.is_none());
}
