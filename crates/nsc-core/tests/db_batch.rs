use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewBatch, NewDataAsset, NewModelConfig, NewTransformationNovel, NewUpload,
    OnFailurePolicy, TransformMode,
};

fn setup_tn(db: &Db) -> i64 {
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x".into(),
        original_text: "正".into(),
        word_count: 0,
    }).unwrap();
    db.seed_builtin_prompts().unwrap();
    let model_id = db.model_configs().insert(&NewModelConfig {
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
    db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "tn".into(),
        default_model_config_id: Some(model_id),
        default_prompt_id: Some(prompt_id),
        default_mode: Some(TransformMode::Compress),
    }).unwrap()
}

#[test]
fn insert_and_list_batches() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let b1 = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("A".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    let b2 = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("B".into()),
        on_failure_policy: OnFailurePolicy::Terminate,
    }).unwrap();
    let all = db.batches().list_by_tn(tn_id).unwrap();
    assert_eq!(all.len(), 2);
    // DESC 排序:新建的 b2 在前
    assert_eq!(all[0].id, b2);
    assert_eq!(all[0].on_failure_policy, OnFailurePolicy::Terminate);
    assert_eq!(all[1].id, b1);
    assert_eq!(all[1].on_failure_policy, OnFailurePolicy::PauseAndReview);
    // 初始 status 必为 pending
    assert_eq!(all[0].status, BatchStatus::Pending);
}

#[test]
fn set_status_starts_ended_timestamps() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: None,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }).unwrap();
    db.batches().set_status(id, BatchStatus::Running).unwrap();
    let b1 = db.batches().get(id).unwrap().unwrap();
    assert_eq!(b1.status, BatchStatus::Running);
    assert!(b1.started_at.is_some());
    assert!(b1.ended_at.is_none());

    db.batches().set_status(id, BatchStatus::Completed).unwrap();
    let b2 = db.batches().get(id).unwrap().unwrap();
    assert_eq!(b2.status, BatchStatus::Completed);
    assert!(b2.ended_at.is_some());

    // Running 已 set 过的 started_at,再次 Running 不会覆盖
    let first_started = b2.started_at.unwrap();
    db.batches().set_status(id, BatchStatus::Running).unwrap();
    let b3 = db.batches().get(id).unwrap().unwrap();
    assert_eq!(b3.started_at, Some(first_started));
}

#[test]
fn count_by_status() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let a = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: None,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }).unwrap();
    db.batches().set_status(a, BatchStatus::Running).unwrap();
    let c = db.batches().count_by_status(tn_id).unwrap();
    assert_eq!(c.running, 1);
    assert_eq!(c.pending, 1);
    assert_eq!(c.completed, 0);
}

#[test]
fn update_label_and_policy() {
    let db = Db::open_in_memory().unwrap();
    let tn_id = setup_tn(&db);
    let id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("orig".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    let mut b = db.batches().get(id).unwrap().unwrap();
    b.label = Some("new".into());
    b.on_failure_policy = OnFailurePolicy::Terminate;
    db.batches().update(&b).unwrap();
    let after = db.batches().get(id).unwrap().unwrap();
    assert_eq!(after.label.as_deref(), Some("new"));
    assert_eq!(after.on_failure_policy, OnFailurePolicy::Terminate);
}
