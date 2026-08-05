//! Startup safe-recovery (spec §7).
//!
//! 应用启动时,前一次崩溃留下的 `Running` 工作流 / `Pending` 或 `Running` 章节会被
//! 安全收口:`Running` tc → `Failed`,`Pending` tc → `Skipped`,`Running` batch → `Stopped`
//! 且 `ended_at` 必须有值。不自动重新调用模型 —— 用户进入工作流详情后可主动重试空槽。

use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewBatch, NewChapter, NewDataAsset, NewTransformationChapter,
    NewTransformationNovel, NewUpload, OnFailurePolicy, TransformMode, TransformStatus,
};

/// In-memory fixture:1 upload → 1 data_asset → 1 TN → 3 chapters → 1 batch(running)
/// + 3 tc 行(status = running / pending / pending)。
fn seed_orphan_world() -> (Db, i64) {
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
        title: "TN".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=3 {
        cids.push(db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            idx: i,
            title: format!("Ch {i}"),
            byte_start: 0,
            byte_end: 6,
            word_count: 2,
        }).unwrap());
    }
    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("orphan".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    // 模拟崩溃:batch 状态是 Running,三章 tc 一行 Running + 两行 Pending。
    db.conn.execute(
        "UPDATE batches SET status='running', started_at=?1 WHERE id=?2",
        rusqlite::params![chrono::Utc::now().to_rfc3339(), batch_id],
    ).unwrap();
    db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cids[0],
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: Some(batch_id),
        style_ref_chapter_id: None,
    }).unwrap();
    let t_running_id = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cids[1],
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: Some(batch_id),
        style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cids[2],
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: Some(batch_id),
        style_ref_chapter_id: None,
    }).unwrap();
    // 把第一行改 Running。
    db.transformation_chapters().mark_running(t_running_id).unwrap();
    (db, batch_id)
}

#[test]
fn startup_recovery_marks_running_chapters_failed_and_pending_skipped_and_batch_stopped() {
    let (db, batch_id) = seed_orphan_world();

    nsc_core::startup_recovery::run(&db.conn).unwrap();

    let tcs = db.transformation_chapters().list_by_batch(batch_id).unwrap();
    let statuses: Vec<TransformStatus> = tcs.iter().map(|t| t.status).collect();
    // 全部 settled:不再有 Running / Pending。
    assert!(
        !statuses.contains(&TransformStatus::Running),
        "running tc 应收口:actual {statuses:?}"
    );
    assert!(
        !statuses.contains(&TransformStatus::Pending),
        "pending tc 应收口:actual {statuses:?}"
    );
    assert!(
        statuses.contains(&TransformStatus::Failed),
        "原 running tc 应为 Failed:actual {statuses:?}"
    );
    assert!(
        statuses.contains(&TransformStatus::Skipped),
        "原 pending tc 应为 Skipped:actual {statuses:?}"
    );

    // Running tc 的 error 必须含"进程中断"线索。
    let running_tc = tcs.iter().find(|t| matches!(t.status, TransformStatus::Failed)).unwrap();
    assert!(
        running_tc.error.as_deref().unwrap_or_default().contains("进程中断"),
        "Failed tc 错误说明应含\"进程中断\":actual {:?}",
        running_tc.error,
    );
    assert!(running_tc.completed_at.is_some(), "completed_at 必须设上");

    // batch → Stopped,ended_at 必须有值。
    let b = db.batches().get(batch_id).unwrap().unwrap();
    assert!(matches!(b.status, BatchStatus::Stopped), "batch 应收 Stopped:actual {:?}", b.status);
    assert!(b.ended_at.is_some(), "batch.ended_at 必须设上");
}

#[test]
fn startup_recovery_is_idempotent_on_second_run() {
    let (db, batch_id) = seed_orphan_world();
    nsc_core::startup_recovery::run(&db.conn).unwrap();

    // 抓快照
    let snap_statuses: Vec<TransformStatus> = db.transformation_chapters()
        .list_by_batch(batch_id).unwrap()
        .iter().map(|t| t.status).collect();

    nsc_core::startup_recovery::run(&db.conn).unwrap();

    let after_statuses: Vec<TransformStatus> = db.transformation_chapters()
        .list_by_batch(batch_id).unwrap()
        .iter().map(|t| t.status).collect();
    assert_eq!(snap_statuses, after_statuses, "二次 recovery 必须不翻转任何状态");

    let snap_err = db.transformation_chapters()
        .list_by_batch(batch_id).unwrap()
        .iter().find(|t| matches!(t.status, TransformStatus::Failed))
        .unwrap().error.clone();
    let after_err = db.transformation_chapters()
        .list_by_batch(batch_id).unwrap()
        .iter().find(|t| matches!(t.status, TransformStatus::Failed))
        .unwrap().error.clone();
    assert_eq!(snap_err, after_err, "二次 recovery 不应改写 error 信息");

    let snap_ended = db.batches().get(batch_id).unwrap().unwrap().ended_at;
    let after_ended = db.batches().get(batch_id).unwrap().unwrap().ended_at;
    assert_eq!(snap_ended, after_ended, "ended_at 必须保持稳定");
}

#[test]
fn startup_recovery_skips_non_running_batches_and_settled_tc() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(),
        byte_size: 0, file_path: "/tmp/x.txt".into(),
        original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "TN".into(),
        default_model_config_id: None, default_prompt_id: None, default_mode: None,
    }).unwrap();
    let cid = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 1, title: "Ch 1".into(),
        byte_start: 0, byte_end: 6, word_count: 2,
    }).unwrap();

    // 第一个 batch:已 Stopped,带一个 Done tc ── 不应被 recovery 触碰。
    let stopped_batch = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    let prior_ended = chrono::Utc::now().to_rfc3339();
    db.conn.execute(
        "UPDATE batches SET status='stopped', started_at=?1, ended_at=?2 WHERE id=?3",
        rusqlite::params![prior_ended.clone(), prior_ended, stopped_batch],
    ).unwrap();
    let t_done = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cid,
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: Some(stopped_batch), style_ref_chapter_id: None,
    }).unwrap();
    let prior_done_content = "DONE-CONTENT";
    let prior_tokens_in = 42;
    let prior_tokens_out = 24;
    db.transformation_chapters().mark_done(
        t_done,
        prior_done_content.into(),
        prior_tokens_in,
        prior_tokens_out,
    ).unwrap();
    let prior_completed_at = db.transformation_chapters().get(t_done).unwrap().unwrap().completed_at;

    // 第二个 batch:Running,带一个 Running tc ── 应被收口。
    let running_batch = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();
    let cid2 = db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 2, title: "Ch 2".into(),
        byte_start: 0, byte_end: 6, word_count: 2,
    }).unwrap();
    db.conn.execute(
        "UPDATE batches SET status='running', started_at=?1 WHERE id=?2",
        rusqlite::params![chrono::Utc::now().to_rfc3339(), running_batch],
    ).unwrap();
    let t_running = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cid2,
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: Some(running_batch), style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_running(t_running).unwrap();

    nsc_core::startup_recovery::run(&db.conn).unwrap();

    // Stopped batch + Done tc 完全不动。
    let stopped_b = db.batches().get(stopped_batch).unwrap().unwrap();
    assert!(
        matches!(stopped_b.status, BatchStatus::Stopped),
        "原 Stopped batch 状态必须保留,实际 {:?}", stopped_b.status
    );
    assert_eq!(
        stopped_b.ended_at.unwrap().to_rfc3339(),
        prior_ended,
        "原 Stopped batch ended_at 必须保留"
    );
    let done_tc = db.transformation_chapters().get(t_done).unwrap().unwrap();
    assert_eq!(done_tc.status, TransformStatus::Done, "Done tc 状态必须保留");
    assert_eq!(done_tc.result_content.as_deref(), Some(prior_done_content));
    assert_eq!(done_tc.tokens_in, Some(prior_tokens_in));
    assert_eq!(done_tc.tokens_out, Some(prior_tokens_out));
    assert_eq!(done_tc.completed_at, prior_completed_at, "完成时间戳必须保留");

    // Running batch + Running tc 收口。
    let running_b = db.batches().get(running_batch).unwrap().unwrap();
    assert!(
        matches!(running_b.status, BatchStatus::Stopped),
        "原 Running batch 应收 Stopped:{:?}", running_b.status
    );
    assert!(running_b.ended_at.is_some());
    let recovered_tc = db.transformation_chapters().get(t_running).unwrap().unwrap();
    assert_eq!(
        recovered_tc.status, TransformStatus::Failed,
        "Running tc 应收 Failed"
    );
    assert!(recovered_tc.error.as_deref().unwrap_or_default().contains("进程中断"));
}
