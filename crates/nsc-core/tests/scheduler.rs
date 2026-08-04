//! BatchScheduler 集成测试（in-memory DB）。
//! Slice 4 范围：frontier SQL + style_ref + create_batch + on_chapter_done 派发链。
//! Slice 5 增量：on_failure_policy 三分支 + resume。

use std::sync::Arc;

use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewBatch, NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
    NewTransformationNovel, NewUpload, OnFailurePolicy, ResumeAction, TransformMode, TransformStatus,
};
use nsc_core::transformer::{BatchScheduler, JobQueue};

/// 假 AI provider —— 直接把 user content 作为 response 返还。
/// 用于不真发 HTTP 的批调度测试。
struct EchoProvider;
#[async_trait]
impl AiProvider for EchoProvider {
    async fn chat(&self, req: ChatRequest) -> nsc_core::error::Result<ChatResponse> {
        let user = req
            .messages
            .iter()
            .find(|m| matches!(m.role, nsc_core::ai::Role::User))
            .map(|m| m.content.clone())
            .unwrap_or_default();
        Ok(ChatResponse {
            content: format!("ECHO:{user}"),
            tokens_in: user.len() as i32,
            tokens_out: user.len() as i32,
        })
    }
}

/// 用假 provider 构造 JobQueue + Scheduler pair。
fn build_pair(db_path: std::path::PathBuf) -> (Arc<JobQueue>, Arc<BatchScheduler>) {
    let path_for_factory = db_path.clone();
    let queue = Arc::new(JobQueue::new(
        1,
        move || Db::open(&path_for_factory),
        |_cfg| -> Box<dyn AiProvider> { Box::new(EchoProvider) },
    ));
    queue.set_notifier(Arc::new(|_tid, _success, _err| {}));
    let scheduler = Arc::new(BatchScheduler::new(db_path, queue.clone()));
    (queue, scheduler)
}

fn seed_with_chapters(n: usize) -> (tempfile::TempDir, std::path::PathBuf, Db, i64, i64, Vec<i64>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sched.db");
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();

    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    // seed 一个真实 model_config,让 tn.default_model_config_id 有合法 FK。
    let cfg_id = db.model_configs().insert(&NewModelConfig {
        name: "mock".into(),
        base_url: "http://localhost".into(),
        api_key: "k".into(),
        model: "m".into(),
        max_tokens: None,
        temperature: None,
        concurrency: 1,
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".to_string(),
        default_model_config_id: Some(cfg_id),
        default_prompt_id: Some(1),
        default_mode: Some(TransformMode::Compress),
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: i as i32,
            title: format!("Ch {i}"),
            byte_start: 0, byte_end: 6, word_count: 2,
        }).unwrap();
        cids.push(cid);
    }
    (dir, path, db, tn_id, da_id, cids)
}

#[test]
fn frontier_chapter_id_returns_prev_done() {
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(3);
    // tc1 done on ch1
    let t1 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[0],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t1, "OK1".into(), 10, 8).unwrap();

    // 直接走 SQL helper（用 db.conn）
    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[1]],
        |row| row.get(0),
    ).ok();
    assert_eq!(cid, Some(cids[0]));
}

#[test]
fn frontier_chapter_id_returns_none_when_no_prev_done() {
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(2);

    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[1]],
        |row| row.get(0),
    ).ok();
    assert_eq!(cid, None);
}

#[test]
fn frontier_skips_idx_in_between() {
    // ch1 done, ch2 pending, ch3 done —— frontier for ch4 应是 ch3（不是 ch1）
    let (_dir, _path, db, tn_id, _da, cids) = seed_with_chapters(4);
    let t1 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[0],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t1, "OK1".into(), 10, 8).unwrap();
    let t3 = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id, chapter_id: cids[2],
        mode: TransformMode::Compress, prompt_id: 1, model_config_id: 1,
        ctx_prev_original: 0, ctx_prev_transformed: 0, ctx_next_original: 0,
        batch_id: None, style_ref_chapter_id: None,
    }).unwrap();
    db.transformation_chapters().mark_done(t3, "OK3".into(), 10, 8).unwrap();

    let cid: Option<i64> = db.conn.query_row(
        "SELECT c.id FROM transformation_chapters tc \
         JOIN chapters c ON c.id = tc.chapter_id \
         WHERE tc.transformation_novel_id = ?1 AND tc.status = 'done' \
           AND c.idx < (SELECT idx FROM chapters WHERE id = ?2) \
         ORDER BY c.idx DESC LIMIT 1",
        rusqlite::params![tn_id, cids[3]],
        |row| row.get(0),
    ).ok();
    assert_eq!(cid, Some(cids[2]));  // ch3（不是 ch1）
}
fn seed_batch_world(path: &std::path::Path, policy: OnFailurePolicy) -> (Db, i64, Vec<i64>) {
    let db = Db::open(path).unwrap();
    db.seed_builtin_prompts().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "x.txt".into(), byte_size: 0,
        file_path: "/tmp/x.txt".into(), original_text: "正文".into(), word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id, title: "DA".into() }).unwrap();
    let cfg_id = db.model_configs().insert(&NewModelConfig {
        name: "mock".into(), base_url: "http://localhost".into(), api_key: "k".into(),
        model: "m".into(), max_tokens: None, temperature: None, concurrency: 1,
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "TN".to_string(),
        default_model_config_id: Some(cfg_id),
        default_prompt_id: Some(1),
        default_mode: Some(TransformMode::Compress),
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=3 {
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id, idx: i, title: format!("C{i}").into(),
            byte_start: 0, byte_end: 2, word_count: 1,
        }).unwrap();
        cids.push(cid);
    }
    drop(db);
    // 重新打开,避免借用冲突
    let db = Db::open(path).unwrap();
    // 拿回 tn_id（实际就是上面的）
    let _ = policy;
    (db, tn_id, cids)
}

#[test]
fn pause_and_review_does_not_advance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pause.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::PauseAndReview);
    let (queue, scheduler) = build_pair(path.clone());

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }, vec![cids[0], cids[1]]).unwrap();
    assert_eq!(batch.status, BatchStatus::Running);

    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    // 模拟 c1 失败
    db.transformation_chapters().mark_failed(tids[0], "fake error".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "fake error".into()).unwrap();

    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Paused);
    let t2_status = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
    assert_eq!(t2_status, TransformStatus::Pending);

    // resume(retry c1) → batch 转 Running
    let _ = scheduler.resume(batch.id, ResumeAction::Retry(tids[0])).unwrap();
    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Running);

    let _ = queue;
}

#[test]
fn terminate_cancels_remaining() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("term.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::Terminate);
    let (queue, scheduler) = build_pair(path.clone());

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::Terminate,
    }, vec![cids[0], cids[1]]).unwrap();

    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    db.transformation_chapters().mark_failed(tids[0], "boom".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "boom".into()).unwrap();

    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Terminated);
    let t2_status = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
    assert_eq!(t2_status, TransformStatus::Cancelled);

    let _ = queue;
}

#[test]
fn skip_failed_marks_skipped_and_keeps_running() {
    // skip_failed 不依赖 worker —— 直接构造 batch + 手动调 on_chapter_failed。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skip.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::SkipFailed);
    let (queue, scheduler) = build_pair(path.clone());

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }, vec![cids[0], cids[1]]).unwrap();

    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    // c1 失败 → 应标 skipped,batch 仍 Running
    db.transformation_chapters().mark_failed(tids[0], "boom".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "boom".into()).unwrap();

    let t1 = db.transformation_chapters().get(tids[0]).unwrap().unwrap();
    assert_eq!(t1.status, TransformStatus::Skipped);
    assert_eq!(t1.error.as_deref(), Some("boom"));

    let b = db.batches().get(batch.id).unwrap().unwrap();
    // SkipFailed 后会 advance_batch 派下一章；此时 c2 还没准备好，
    // 这里只验证 batch 状态（应该 Running 或 Completed 取决于派发结果）。
    // 关键断言:c1 skipped,batch 不为 Paused/Terminated。
    assert!(matches!(b.status, BatchStatus::Running | BatchStatus::Completed),
            "expected Running or Completed, got {:?}", b.status);

    let _ = queue;
}
