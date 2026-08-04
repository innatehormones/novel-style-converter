use nsc_core::db::Db;
use nsc_core::models::{
    NewChapter, NewDataAsset, NewTransformationChapter, NewTransformationNovel, NewUpload,
    TransformMode, TransformStatus,
};

fn setup() -> (Db, i64, i64) {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x.txt".into(),
        original_text: "正文".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id, title: "DA".into(),
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "N".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    let cid = db.chapters().insert(&NewChapter {
        data_asset_id: da_id,
        idx: 1,
        title: "Ch 1".into(),
        byte_start: 0,
        byte_end: 6,
        word_count: 2,
    }).unwrap();
    (db, tn_id, cid)
}

#[test]
fn insert_pending_then_mark_done() {
    let (db, tn_id, cid) = setup();
    let id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cid,
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap();

    db.transformation_chapters().mark_running(id).unwrap();
    db.transformation_chapters().mark_done(id, "RES".into(), 100, 80).unwrap();

    let t = db.transformation_chapters().get(id).unwrap().unwrap();
    assert_eq!(t.status, TransformStatus::Done);
    assert_eq!(t.result_content.as_deref(), Some("RES"));
    assert_eq!(t.tokens_in, Some(100));
    assert_eq!(t.tokens_out, Some(80));
}

#[test]
fn mark_failed_records_error() {
    let (db, tn_id, cid) = setup();
    let id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cid,
        mode: TransformMode::Style,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap();

    db.transformation_chapters().mark_failed(id, "boom".into()).unwrap();

    let t = db.transformation_chapters().get(id).unwrap().unwrap();
    assert_eq!(t.status, TransformStatus::Failed);
    assert_eq!(t.error.as_deref(), Some("boom"));
}