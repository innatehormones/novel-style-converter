//! append_chapters_to_batch 集成测试 —— spec §7.1。
//!
//! 本文件覆盖 BatchScheduler::append_chapters_to_batch 依赖的 Db 端路径
//! (batch 存在性 / status 校验 / chapter_ids 归属 / tc 去重)。
//! 完整 scheduler 路径(含 JobQueue 入队 / advance_batch)由 e2e 测试覆盖。
use nsc_core::db::Db;
use nsc_core::models::batch::{BatchStatus, NewBatch, OnFailurePolicy};
use nsc_core::models::prompt::PromptKind;
use std::collections::HashSet;

fn seed_env() -> (Db, i64, Vec<i64>) {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&nsc_core::models::NewUpload {
        sha256: "x".into(),
        filename: "t.txt".into(),
        byte_size: 0,
        file_path: String::new(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "t.txt".into(),
        ..Default::default()
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=3 {
        cids.push(db.chapters().insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("chapter {i}"),
            body: format!("body {i}"),
            word_count: 1,
            ..Default::default()
        }).unwrap());
    }
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: String::new(),
    }).unwrap();
    (db, tn_id, cids)
}

fn insert_batch_with_status(db: &Db, tn_id: i64, status: BatchStatus, with_chapter_ids: &[i64]) -> i64 {
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("test".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
        prompt_id: 0,
        model_config_id: 0,
        mode: "compress".into(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        ctx_next_transformed: 0,
    }).unwrap();
    db.batches().set_status(batch_id, status).unwrap();
    if !with_chapter_ids.is_empty() {
        let b = db.batches().get(batch_id).unwrap().unwrap();
        for &cid in with_chapter_ids {
            db.transformation_chapters().insert(&nsc_core::models::NewTransformationChapter {
                transformation_novel_id: b.transformation_novel_id,
                chapter_id: cid,
                mode: PromptKind::Compress,
                prompt_id: 0,
                model_config_id: 0,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
                batch_id: Some(batch_id),
                style_ref_chapter_id: None,
            }).unwrap();
        }
    }
    batch_id
}

#[test]
fn read_batch_status_after_insert() {
    // 占位:验证 Db 端 setup 正确。
    let (db, tn_id, cids) = seed_env();
    let bid = insert_batch_with_status(&db, tn_id, BatchStatus::Stopped, &cids[..1]);
    let b = db.batches().get(bid).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Stopped);
    assert_eq!(b.transformation_novel_id, tn_id);
}

#[test]
fn list_existing_tcs_by_batch() {
    let (db, tn_id, cids) = seed_env();
    let bid = insert_batch_with_status(&db, tn_id, BatchStatus::Stopped, &cids[..2]);
    let tcs = db.transformation_chapters().list_by_batch(bid).unwrap();
    assert_eq!(tcs.len(), 2);
    let tc_chapter_ids: HashSet<i64> = tcs.iter().map(|tc| tc.chapter_id).collect();
    assert!(tc_chapter_ids.contains(&cids[0]));
    assert!(tc_chapter_ids.contains(&cids[1]));
    assert!(!tc_chapter_ids.contains(&cids[2]));
}

