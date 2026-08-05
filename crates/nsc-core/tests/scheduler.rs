//! BatchScheduler 集成测试（in-memory DB）。
//! Slice 4 范围：frontier SQL + style_ref + create_batch + on_chapter_done 派发链。
//! Slice 5 增量：on_failure_policy 三分支 + resume。

use std::sync::Arc;

use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::error::Error;
use nsc_core::models::{
    BatchStatus, NewBatch, NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
    NewTransformationNovel, NewUpload, OnFailurePolicy, TransformMode, TransformStatus,
};
use nsc_core::transformer::{BatchScheduler, JobQueue, WorkflowCreate};

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
fn failure_marks_failed_and_advances() {
    // Task 4 后 on_chapter_failed 不再按 policy 分流:失败一律 Failed + advance_batch。
    // c1 失败后 batch 仍 Running(noop notifier 不会触发 on_chapter_done),
    // t2 由 advance_batch 派发进入 JobQueue 但尚未完成 → status 还是 Pending/Running。
    // 单行为已被 Task 4 收敛,本测试不区分 OnFailurePolicy:policy 参数仅做种子注入。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fail.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::PauseAndReview);
    let (queue, scheduler) = build_pair(path.clone());

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }, vec![cids[0], cids[1]], nsc_core::transformer::BatchOverrides::default()).unwrap();
    assert_eq!(batch.status, BatchStatus::Running);

    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    // 模拟 c1 失败:worker 先 mark_failed,scheduler 回调 on_chapter_failed。
    db.transformation_chapters().mark_failed(tids[0], "fake error".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "fake error".into()).unwrap();

    let t1 = db.transformation_chapters().get(tids[0]).unwrap().unwrap();
    assert_eq!(t1.status, TransformStatus::Failed);
    // 错误必须保留(旧 SkipFailed 分支会清掉 error)。
    assert_eq!(t1.error.as_deref(), Some("fake error"));
    assert!(t1.result_content.is_none(), "失败时 result_content 必须清空");

    let b = db.batches().get(batch.id).unwrap().unwrap();
    // 单一行为:不为 Paused/Terminated/Stopped,只有 advance_batch 派下一章
    // (no notifier → batch 不会 finalize)。
    assert!(matches!(b.status, BatchStatus::Running), "got {:?}", b.status);

    let _ = queue;
}

#[test]
fn failure_does_not_cancel_remaining_tc() {
    // 旧 Terminate 分支:同 batch pending → cancelled。新行为:tc1 Failed, tc2 仍 pending。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("term.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::Terminate);
    let (queue, scheduler) = build_pair(path.clone());

    let batch = scheduler.create_batch(NewBatch {
        transformation_novel_id: tn_id, label: None,
        on_failure_policy: OnFailurePolicy::Terminate,
    }, vec![cids[0], cids[1]], nsc_core::transformer::BatchOverrides::default()).unwrap();

    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();

    db.transformation_chapters().mark_failed(tids[0], "boom".into()).unwrap();
    scheduler.on_chapter_failed(tids[0], "boom".into()).unwrap();

    let t1 = db.transformation_chapters().get(tids[0]).unwrap().unwrap();
    assert_eq!(t1.status, TransformStatus::Failed);
    let t2_status = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
    // 旧行为是 Cancelled,新行为:advance_batch 派下一章,tc2 已不在 Pending。
    // 但 worker 没回调(noop notifier),tc2 状态还是 pending/running。
    assert!(
        matches!(t2_status, TransformStatus::Pending | TransformStatus::Running),
        "got {t2_status:?}"
    );

    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_ne!(b.status, BatchStatus::Terminated);

    let _ = queue;
}

#[test]
fn dispatch_batch_fills_chapters_and_advances() {
    // create_batch_row → dispatch_batch:走"先建空 batch,后派"的 2 步流程。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("dispatch.db");
    let (db, tn_id, cids) = seed_batch_world(&path, OnFailurePolicy::PauseAndReview);
    let (queue, scheduler) = build_pair(path.clone());

    let batch_id = db.batches().insert(&NewBatch {
        transformation_novel_id: tn_id,
        label: Some("manual".into()),
        on_failure_policy: OnFailurePolicy::PauseAndReview,
    }).unwrap();

    let b = scheduler.dispatch_batch(
        batch_id,
        nsc_core::transformer::BatchOverrides::default(),
    ).unwrap();
    assert_eq!(b.id, batch_id);
    assert_eq!(b.status, BatchStatus::Running);

    // tc 行应自动落 N 行（cids.len()）
    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch_id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(tids.len(), cids.len(), "应给每个 chapter 落一行 tc");

    // 重复 dispatch → ValidationError（已不是 Pending）
    let err = scheduler.dispatch_batch(
        batch_id,
        nsc_core::transformer::BatchOverrides::default(),
    ).unwrap_err();
    assert!(format!("{err}").contains("不是 Pending"), "got: {err}");

    let _ = queue;
}

/// 原子创建工作流 → 立刻 Running，且 tc 行数与 slot 数都 = chapter_ids.len()。
#[test]
fn create_workflow_is_atomic_and_initial_running() {
    let (_dir, path, _db, tn_id, _da, cids) = seed_with_chapters(3);
    let (_queue, sched) = build_pair(path.clone());

    let batch = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: Some("v1".into()),
        chapter_ids: vec![cids[0], cids[1]],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap();

    assert_eq!(batch.status, BatchStatus::Running);

    let db = Db::open(&path).unwrap();
    // tc 数 == chapter_ids 数
    let tc_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id = ?1",
        rusqlite::params![batch.id], |r| r.get(0)
    ).unwrap();
    assert_eq!(tc_count, 2);
    // slot 数 == chapter_ids 数
    let slot_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM workflow_result_chapters wrc \
         JOIN workflow_results wr ON wr.id = wrc.workflow_result_id \
         WHERE wr.batch_id = ?1",
        rusqlite::params![batch.id], |r| r.get(0)
    ).unwrap();
    assert_eq!(slot_count, 2);
}

/// 空 chapter_ids → ValidationError，事务回滚：batches 表零行。
#[test]
fn create_workflow_empty_chapter_ids_rejected() {
    let (_dir, path, _db, tn_id, _da, _cids) = seed_with_chapters(3);
    let (_queue, sched) = build_pair(path.clone());

    let err = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: vec![],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");

    let db = Db::open(&path).unwrap();
    let batch_count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM batches", [], |r| r.get(0)
    ).unwrap();
    assert_eq!(batch_count, 0);
}

/// chapter 归属不一致 → ValidationError，所有写表零行（事务原子回滚）。
#[test]
fn create_workflow_chapter_not_in_data_asset_rejected() {
    let (_dir, path, _db, tn_id, _da, cids) = seed_with_chapters(2);
    // 第二个 data_asset,塞一个 chapter 不属于 tn 关联的 da。
    let db = Db::open(&path).unwrap();
    let upload2 = db.uploads().insert(&NewUpload {
        sha256: "h2".into(), filename: "y.txt".into(), byte_size: 0,
        file_path: "/tmp/y.txt".into(), original_text: "".into(), word_count: 0,
    }).unwrap();
    let da2 = db.data_assets().insert(&NewDataAsset { upload_id: upload2, title: "DA2".into() }).unwrap();
    let foreign_cid = db.chapters().insert(&NewChapter {
        data_asset_id: da2, idx: 1, title: "X".into(),
        byte_start: 0, byte_end: 1, word_count: 1,
    }).unwrap();
    drop(db);

    let (_queue, sched) = build_pair(path.clone());
    let err = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: vec![cids[0], foreign_cid],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");

    let db = Db::open(&path).unwrap();
    let n: i64 = db.conn.query_row("SELECT COUNT(*) FROM batches", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
    let n: i64 = db.conn.query_row("SELECT COUNT(*) FROM workflow_results", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
    let n: i64 = db.conn.query_row("SELECT COUNT(*) FROM transformation_chapters", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
    let n: i64 = db.conn.query_row("SELECT COUNT(*) FROM workflow_result_chapters", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

/// prompt.kind 与 mode 不一致 → ValidationError，不写任何 batch 行。
#[test]
fn create_workflow_prompt_kind_mode_mismatch_rejected() {
    let (_dir, path, db, tn_id, _da, cids) = seed_with_chapters(2);
    // 取 id=2 那个 Style prompt,与 Compress mode 故意不一致。
    let style_prompt_id: i64 = db.conn.query_row(
        "SELECT id FROM prompts WHERE kind = 'style' LIMIT 1", [], |r| r.get(0)
    ).unwrap();
    drop(db);

    let (_queue, sched) = build_pair(path.clone());
    let err = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: vec![cids[0]],
        prompt_id: style_prompt_id,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap_err();
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");

    let db = Db::open(&path).unwrap();
    let n: i64 = db.conn.query_row("SELECT COUNT(*) FROM batches", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

/// 端到端:首章失败 → 标 Failed,后续章节仍 dispatch 并完成,batch 自然收尾为 Stopped。
/// 用 noop notifier + 手动驱动 scheduler 回调(避免 notify 锁重入死锁)。
#[test]
fn failed_chapter_marks_failed_and_next_chapter_runs_then_workflow_stops() {
    let (_dir, path, _db, tn_id, _da, cids) = seed_with_chapters(2);
    let (_queue, sched) = build_pair(path.clone());

    let batch = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: vec![cids[0], cids[1]],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap();
    assert_eq!(batch.status, BatchStatus::Running);

    let db = Db::open(&path).unwrap();
    let tids: Vec<i64> = db.conn.prepare(
        "SELECT id FROM transformation_chapters WHERE batch_id=?1 ORDER BY id ASC"
    ).unwrap().query_map(rusqlite::params![batch.id], |r| r.get(0))
    .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    drop(db);

    // 等 worker 完成 c1(EchoProvider 同步返回 done)再覆盖。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let db = Db::open(&path).unwrap();
        let s = db.transformation_chapters().get(tids[0]).unwrap().unwrap().status;
        drop(db);
        if matches!(s, TransformStatus::Done) { break; }
        if std::time::Instant::now() > deadline {
            panic!("c1 5s 内未被 worker 标记 Done,当前 {s:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // 模拟 worker:tc1 mark_failed,scheduler 回调 on_chapter_failed → 派 tc2。
    let db = Db::open(&path).unwrap();
    db.transformation_chapters().mark_failed(tids[0], "boom".into()).unwrap();
    drop(db);
    sched.on_chapter_failed(tids[0], "boom".into()).unwrap();

    // 等 worker 跑完 c2,然后调 on_chapter_done 收尾。
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let db = Db::open(&path).unwrap();
        let s = db.transformation_chapters().get(tids[1]).unwrap().unwrap().status;
        drop(db);
        if matches!(s, TransformStatus::Done) { break; }
        if std::time::Instant::now() > deadline {
            panic!("c2 5s 内未被 worker 标记 Done,当前 {s:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    sched.on_chapter_done(tids[1]).unwrap();

    let db = Db::open(&path).unwrap();
    let statuses: Vec<TransformStatus> = db.transformation_chapters()
        .list_by_batch(batch.id).unwrap()
        .iter().map(|t| t.status).collect();
    assert!(statuses.contains(&TransformStatus::Failed), "first chapter must be Failed; got {statuses:?}");
    assert!(statuses.contains(&TransformStatus::Done), "second chapter must be Done; got {statuses:?}");
    assert_eq!(statuses.len(), 2);
    let final_status = db.batches().get(batch.id).unwrap().unwrap().status;
    assert_eq!(final_status, BatchStatus::Stopped);
}

/// 回归 guard:1 章 batch 的唯一章节失败后 active=0,batch 必须收尾为 Stopped
/// (若有人把 'failed' 加回 maybe_finalize_batch 的 active 集合,此测试立即失败)。
#[test]
fn last_chapter_failure_finalizes_batch_as_stopped() {
    let (_dir, path, _db, tn_id, _da, cids) = seed_with_chapters(1);
    let (_queue, sched) = build_pair(path.clone());

    let batch = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: vec![cids[0]],
        prompt_id: 1,
        model_config_id: 1,
        mode: TransformMode::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }).unwrap();

    let tid = {
        let db = Db::open(&path).unwrap();
        db.transformation_chapters().list_by_batch(batch.id).unwrap()[0].id
    };

    sched.on_chapter_failed(tid, "simulated LLM error".into()).unwrap();

    let db = Db::open(&path).unwrap();
    let tc = db.transformation_chapters().get(tid).unwrap().unwrap();
    assert_eq!(tc.status, TransformStatus::Failed);
    let b = db.batches().get(batch.id).unwrap().unwrap();
    assert_eq!(b.status, BatchStatus::Stopped, "1-chapter batch whose only chapter fails must finalize as Stopped");
    assert!(b.ended_at.is_some(), "ended_at must be set");
}
