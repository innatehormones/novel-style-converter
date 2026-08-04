use nsc_core::db::Db;
use nsc_core::models::{
    NewChapter, NewDataAsset, NewTransformationChapter, NewTransformationNovel, NewUpload,
    Prompt, PromptKind, TransformMode,
};
use nsc_core::prompts;

#[test]
fn seed_inserts_only_when_empty() {
    let db = Db::open_in_memory().unwrap();

    db.seed_builtin_prompts().unwrap();
    let first = db.prompts().list().unwrap();
    assert_eq!(first.len(), prompts::builtin_prompts().len());
    assert!(first.iter().all(|p| p.is_builtin));

    db.seed_builtin_prompts().unwrap();
    let second = db.prompts().list().unwrap();
    assert_eq!(second.len(), first.len());
}

/// 准备 1 个 data_asset + 1 个 transformation_novel + 1 个 chapter,
/// 返回 (tn_id, chapter_id)。供 count_by_prompt 测试用。
fn setup_tn(db: &Db) -> (i64, i64) {
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x.txt".into(),
        original_text: "正文".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: "DA".into(),
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
    (tn_id, cid)
}

#[test]
fn count_by_prompt_returns_ref_count() {
    let db = Db::open_in_memory().unwrap();
    db.seed_builtin_prompts().unwrap();
    let prompt_a = db.prompts().list().unwrap()[0].id;
    let prompt_b = db.prompts().insert(&Prompt {
        id: 0,
        name: "user".into(),
        kind: PromptKind::Compress,
        template: "x".into(),
        is_builtin: false,
    }).unwrap();

    let (tn_id, cid) = setup_tn(&db);
    for _ in 0..3 {
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id: cid,
            mode: TransformMode::Compress,
            prompt_id: prompt_a,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        }).unwrap();
    }
    for _ in 0..2 {
        db.transformation_chapters().insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id: cid,
            mode: TransformMode::Style,
            prompt_id: prompt_b,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        }).unwrap();
    }

    assert_eq!(db.prompts().count_by_prompt(prompt_a).unwrap(), 3);
    assert_eq!(db.prompts().count_by_prompt(prompt_b).unwrap(), 2);
}

#[test]
fn count_by_prompt_zero_for_unused() {
    let db = Db::open_in_memory().unwrap();
    db.seed_builtin_prompts().unwrap();
    let pid = db.prompts().list().unwrap()[0].id;
    assert_eq!(db.prompts().count_by_prompt(pid).unwrap(), 0);
}
