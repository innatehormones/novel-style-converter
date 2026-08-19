// resume() 同类 std::sync::Mutex 二次 lock 死锁回归测试。
//
// 修复前:Retry / Skip / Terminate 三分支 commit 后,函数末尾 self.db.batches().get(batch_id)
// 触发第二次 lock(),std::sync::Mutex 非可重入 → 死锁。
// 修复后:per-branch scope 隔离,_bsg 在 match arm 末尾 drop,函数末尾新锁正常拿到。
//
// 测法:直接 SQL 构造 paused batch + failed tc(跳过 AI 跑批),然后调 sched.resume(),
// 用 std::thread::spawn + recv_timeout 包死锁检测 —— 修复前会卡死到测试 timeout,
// 修复后立即返回。三个分支各一个测试。

use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel,
    NewUpload, ResumeAction,
};
use nsc_core::transformer::{BatchScheduler, JobQueue};

/// 200ms 后返回 ECHO_CONTENT —— worker 真跑可观测。
struct SlowEchoProvider;
#[async_trait]
impl AiProvider for SlowEchoProvider {
    async fn chat(&self, _req: ChatRequest) -> nsc_core::error::Result<ChatResponse> {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(ChatResponse {
            content: "ECHO_CONTENT".to_string(),
            tokens_in: 5,
            tokens_out: 5,
        })
    }
}

/// 立即返回 Err —— 用于构造大批章节连续失败的场景。
struct FastFailProvider;
#[async_trait]
impl AiProvider for FastFailProvider {
    async fn chat(&self, _req: ChatRequest) -> nsc_core::error::Result<ChatResponse> {
        Err(nsc_core::error::Error::Other("fast-fail".into()))
    }
}


fn seed_paused_batch_with_failed_tc(
    path: &std::path::Path,
    n: usize,
) -> (i64, i64, Vec<i64>) {
    let db = Db::open(path).unwrap();
    db.seed_builtin_prompts().unwrap();

    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 10,
        file_path: "/tmp/x.txt".into(),
        original_text: "正文段落一段".into(),
        word_count: 6,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "x.txt".into(),
        kind: nsc_core::models::DataAssetKind::Source,
        source_workflow_id: None,
        source_data_asset_id: None,
        note: "".into(),
    }).unwrap();
    let cfg_id = db.model_configs().insert(&NewModelConfig {
        name: "mock".into(),
        base_url: "http://localhost".into(),
        api_key: "k".into(),
        model: "m".into(),
        max_tokens: None,
        max_context: None,
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: "".into(),
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("Ch {i}"),
            body: "正文段落一段".into(),
            word_count: 6,
            source_kind: "original".into(),
            source_chapter_id: None,
        }).unwrap();
        cids.push(cid);
    }

    // 直接 SQL 构造 paused batch + n 个 failed tc(跳过 AI 跑批)。
    let now = chrono::Utc::now().to_rfc3339();
    let batch_id: i64 = db.lock().query_row(
        "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at) \
         VALUES (?1, NULL, ?2, 'paused', ?3, ?3, ?3) RETURNING id",
        rusqlite::params![tn_id, "pause_and_review", now],
        |r| r.get(0),
    ).unwrap();
    for cid in &cids {
        db.lock().execute(
            "INSERT INTO transformation_chapters \
             (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
              batch_id, status, error, completed_at) \
             VALUES (?1, ?2, 'compress', 1, ?3, 0, 0, 0, ?4, 'failed', 'mock_err', ?5)",
            rusqlite::params![tn_id, cid, cfg_id, batch_id, now],
        ).unwrap();
    }
    (tn_id, cfg_id, cids)
}

fn make_sched(path: &std::path::Path) -> Arc<BatchScheduler> {
    let path_for_factory = path.to_path_buf();
    let queue = Arc::new(JobQueue::new(
        2,
        move || Db::open(&path_for_factory),
        |_cfg| -> Box<dyn AiProvider> { Box::new(SlowEchoProvider) },
        Arc::new(nsc_core::recorder::NoopRecorder),
        Arc::new(std::collections::HashSet::<String>::new()),
    ));
    let shared_db = Db::open(path).unwrap();
    let sched = Arc::new(BatchScheduler::new(
        shared_db.clone(),
        queue.clone(),
        Arc::new(|_cfg| -> Box<dyn AiProvider> { Box::new(SlowEchoProvider) }),
        Arc::new(nsc_core::recorder::NoopRecorder),
        Arc::new(std::collections::HashSet::<String>::new()),
    ));
    // 不挂 notifier —— 测试只关心 resume 内部 SQL 路径不死锁,
    // 不依赖 worker 回调收尾(Retry 分支 worker 会真跑,但 resume 本身立即返回,
    // 不等 AI 完成)。
    sched
}

/// 跑 resume 在独立线程,带超时;若超时未返回,说明死锁,返回 Err。
fn run_resume_with_timeout(
    sched: Arc<BatchScheduler>,
    batch_id: i64,
    action: ResumeAction,
    timeout: Duration,
) -> Result<nsc_core::models::Batch, String> {
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    thread::spawn(move || {
        let res = sched.resume(batch_id, action);
        let _ = tx.send((res, started.elapsed()));
    });
    match rx.recv_timeout(timeout) {
        Ok((Ok(b), elapsed)) => {
            eprintln!("[test] resume returned in {elapsed:?}");
            Ok(b)
        }
        Ok((Err(e), elapsed)) => Err(format!("resume returned error after {elapsed:?}: {e}")),
        Err(_) => Err(format!("resume 卡死超过 {timeout:?},疑似死锁未修复")),
    }
}

#[test]
fn resume_terminate_does_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let (_tn_id, _cfg_id, _cids) = seed_paused_batch_with_failed_tc(&path, 3);
    let sched = make_sched(&path);
    let db = Db::open(&path).unwrap();
    let batch_id: i64 = db.lock()
        .query_row("SELECT id FROM batches LIMIT 1", [], |r| r.get(0))
        .unwrap();

    // 5s 保守上限;修复后 resume 应 <100ms 返回
    let updated = run_resume_with_timeout(
        sched.clone(),
        batch_id,
        ResumeAction::Terminate,
        Duration::from_secs(5),
    ).expect("Terminate 必须不卡死");
    assert_eq!(updated.status, BatchStatus::Terminated, "Terminate → batch=Terminated, 实际 {:?}", updated.status);

    // Terminate 只动 PENDING tc(我们的 seed 全是 failed,所以 cancelled_count=0);
    // 原 failed tc 保持 failed 不被 Terminate 误动。
    let db = Db::open(&path).unwrap();
    let counts: (i64, i64) = db.lock().query_row(
        "SELECT \
            COALESCE(SUM(CASE WHEN status='cancelled' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END), 0) \
         FROM transformation_chapters WHERE batch_id=?1",
        rusqlite::params![batch_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(counts, (0, 3), "Terminate 后:0 cancelled, 3 failed(原状态保留), 实际 {counts:?}");
}

#[test]
fn resume_skip_does_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let (_tn_id, _cfg_id, cids) = seed_paused_batch_with_failed_tc(&path, 3);
    let sched = make_sched(&path);
    let db = Db::open(&path).unwrap();
    let batch_id: i64 = db.lock()
        .query_row("SELECT id FROM batches LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let failed_cid = cids[0];

    let updated = run_resume_with_timeout(
        sched.clone(),
        batch_id,
        ResumeAction::Skip(failed_cid),
        Duration::from_secs(5),
    ).expect("Skip 必须不卡死");
    // Skip 分支:该 tc → skipped,batch → running,advance_batch 查无 pending → finalize → Stopped
    assert_eq!(updated.status, BatchStatus::Stopped, "Skip 后 batch 应 Stopped(无 pending → advance_batch finalize), 实际 {:?}", updated.status);

    // Skip 只动指定那 1 个 tc → skipped;其他 2 个保持 failed。
    let db = Db::open(&path).unwrap();
    let counts: (i64, i64) = db.lock().query_row(
        "SELECT \
            COALESCE(SUM(CASE WHEN status='skipped' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END), 0) \
         FROM transformation_chapters WHERE batch_id=?1",
        rusqlite::params![batch_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(counts, (1, 2), "Skip 后:1 skipped(被指定的 tc), 2 failed(其余), 实际 {counts:?}");
}

#[test]
fn resume_retry_does_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let (_tn_id, _cfg_id, cids) = seed_paused_batch_with_failed_tc(&path, 1);
    let sched = make_sched(&path);
    let db = Db::open(&path).unwrap();
    let batch_id: i64 = db.lock()
        .query_row("SELECT id FROM batches LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let failed_cid = cids[0];

    // Retry 分支:tc → pending + dispatch,SlowEchoProvider 200ms 后完成。
    // resume 本身立即返回(不等 AI)。worker 在后台跑,后续若要验证 dispatch,
    // 加 notifier + 轮询 batch=Stopped 即可(此测试不覆盖)。
    let updated = run_resume_with_timeout(
        sched.clone(),
        batch_id,
        ResumeAction::Retry(failed_cid),
        Duration::from_secs(5),
    ).expect("Retry 必须不卡死");
    assert_eq!(updated.status, BatchStatus::Running, "Retry → batch=Running, 实际 {:?}", updated.status);

    // Retry 后该 tc 应离开 failed —— 要么 pending(resume 的 UPDATE 还没被 worker 覆盖),
    // 要么 running(worker 已 mark_running),总之不应再是 failed。
    // 测 pending 是不稳定状态:SlowEchoProvider 之前 worker 会极快 mark_running(几 ms 内),
    // 测试读 DB 时常已错过 pending 窗口。tc 非 failed 即说明 dispatch 成功。
    let db = Db::open(&path).unwrap();
    let failed_count: i64 = db.lock().query_row(
        "SELECT COUNT(*) FROM transformation_chapters WHERE batch_id=?1 AND status='failed'",
        rusqlite::params![batch_id],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(failed_count, 0, "Retry 后该 tc 应已离开 failed, 实际 failed={failed_count}");
}
// ===== 同步递归链爆栈回归 =====
//
// 修复前:SkipFailed 策略下,on_chapter_failed → advance_batch → dispatch → enqueue →
// fire → cb(on_chapter_failed) → advance_batch → ... 是同步链式递归,栈深度 = 章节数。
// Windows 主线程默认栈 2MB,N≥~500 时会 panic("stack overflow")。
//
// 修复后:`fire` 改名 `queue_callback`,只 push envelope 到 `pending_callbacks`,
// worker loop 在 `run_job` 之后 drain 再 invoke —— 栈深度恒为 1(advance_batch 自身的
// dispatch → enqueue → push → return,~10 帧),N 再大也不爆。
//
// 测法:seed paused batch + 800 个 pending tc + skip_failed 策略;
// resume(Retry, first) → batch→running,worker 失败链一路推到最后一个 tc → 全部 skipped → Stopped。

fn seed_paused_batch_with_pending_tcs_skip_failed(
    path: &std::path::Path,
    n: usize,
) -> (i64, i64, Vec<i64>) {
    let db = Db::open(path).unwrap();
    db.seed_builtin_prompts().unwrap();

    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 10,
        file_path: "/tmp/x.txt".into(),
        original_text: "正文段落一段".into(),
        word_count: 6,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id,
        title: "DA".into(),
        source_filename: "x.txt".into(),
        kind: nsc_core::models::DataAssetKind::Source,
        source_workflow_id: None,
        source_data_asset_id: None,
        note: "".into(),
    }).unwrap();
    let cfg_id = db.model_configs().insert(&NewModelConfig {
        name: "mock".into(),
        base_url: "http://localhost".into(),
        api_key: "k".into(),
        model: "m".into(),
        max_tokens: None,
        max_context: None,
        temperature: None,
        disable_thinking: false,
        concurrency: 1,
    }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id,
        title: "TN".into(),
        note: "".into(),
    }).unwrap();
    let mut cids = Vec::new();
    for i in 1..=n {
        let cid = db.chapters().insert(&NewChapter {
            data_asset_id: da_id,
            idx: i as i32,
            title: format!("Ch {i}"),
            body: "正文段落一段".into(),
            word_count: 6,
            source_kind: "original".into(),
            source_chapter_id: None,
        }).unwrap();
        cids.push(cid);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let batch_id: i64 = db.lock().query_row(
        "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at, started_at, ended_at) \
         VALUES (?1, NULL, ?2, 'paused', ?3, ?3, ?3) RETURNING id",
        rusqlite::params![tn_id, "skip_failed", now],
        |r| r.get(0),
    ).unwrap();
    for cid in &cids {
        db.lock().execute(
            "INSERT INTO transformation_chapters \
             (transformation_novel_id, chapter_id, mode, prompt_id, model_config_id, \
              ctx_prev_original, ctx_prev_transformed, ctx_next_original, \
              batch_id, status) \
             VALUES (?1, ?2, 'compress', 1, ?3, 0, 0, 0, ?4, 'pending')",
            rusqlite::params![tn_id, cid, cfg_id, batch_id],
        ).unwrap();
    }
    (tn_id, cfg_id, cids)
}

fn make_sched_with_failing_provider(path: &std::path::Path) -> (Arc<BatchScheduler>, Arc<Db>) {
    let path_for_factory = path.to_path_buf();
    let queue = Arc::new(JobQueue::new(
        2,
        move || Db::open(&path_for_factory),
        |_cfg| -> Box<dyn AiProvider> { Box::new(FastFailProvider) },
        Arc::new(nsc_core::recorder::NoopRecorder),
        Arc::new(std::collections::HashSet::<String>::new()),
    ));
    let shared_db = Db::open(path).unwrap();
    let sched = Arc::new(BatchScheduler::new(
        shared_db.clone(),
        queue.clone(),
        Arc::new(|_cfg| -> Box<dyn AiProvider> { Box::new(FastFailProvider) }),
        Arc::new(nsc_core::recorder::NoopRecorder),
        Arc::new(std::collections::HashSet::<String>::new()),
    ));
    // 设 notifier —— 不挂的话 on_chapter_failed 不会被自动调,advance_batch 也不会被触发,
    // 整个递归链测不出来。
    let cb_sched = sched.clone();
    queue.set_notifier(Arc::new(move |tid, success, error, content| {
        if !success && error.is_none() { return; }
        let res = if success {
            cb_sched.on_chapter_done(tid, content)
        } else {
            cb_sched.on_chapter_failed(tid, error.unwrap_or_default())
        };
        if let Err(_) = res {
        }
    }));
    (sched, shared_db)
}

fn wait_for_batch_status(
    db: &Db,
    batch_id: i64,
    expected: BatchStatus,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let b = db.batches().get(batch_id).map_err(|e| e.to_string())?
            .ok_or_else(|| "batch gone".to_string())?;
        if b.status == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("超时 batch.status 仍 {:?}, 期望 {:?}", b.status, expected));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn skip_failed_many_chapters_does_not_stack_overflow() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    // Windows 主线程默认栈 2MB,~10 帧/章 ≈ 500 章临界;取 800 给点冗余。
    let n: usize = 800;
    let (_tn_id, _cfg_id, cids) = seed_paused_batch_with_pending_tcs_skip_failed(&path, n);
    let (sched, shared_db) = make_sched_with_failing_provider(&path);
    let db = Db::open(&path).unwrap();
    let batch_id: i64 = db.lock()
        .query_row("SELECT id FROM batches LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let first_cid = cids[0];

    // resume 必须不爆栈地返回(栈深度 ≈ 10 帧,不是 n * 10 帧)。
    let updated = run_resume_with_timeout(
        sched.clone(),
        batch_id,
        ResumeAction::Retry(first_cid),
        Duration::from_secs(10),
    ).expect("大批章节 SkipFailed 必须不爆栈");
    assert_eq!(updated.status, BatchStatus::Running, "Retry 后 batch → Running, 实际 {:?}", updated.status);

    // 等 batch 收尾(advance_batch 链式派发到最后一个 pending tc → 全部 skipped → maybe_finalize → Stopped)。
    wait_for_batch_status(&shared_db, batch_id, BatchStatus::Stopped, Duration::from_secs(60))
        .expect("SkipFailed 跑批必须 30s 内完成");

    // 全 n 个 tc 都应 skipped。
    let db = Db::open(&path).unwrap();
    let (skipped, other): (i64, i64) = db.lock().query_row(
        "SELECT \
            COALESCE(SUM(CASE WHEN status='skipped' THEN 1 ELSE 0 END), 0), \
            COALESCE(SUM(CASE WHEN status<>'skipped' THEN 1 ELSE 0 END), 0) \
         FROM transformation_chapters WHERE batch_id=?1",
        rusqlite::params![batch_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).unwrap();
    assert_eq!(skipped, n as i64, "应 {n} 个 tc 都 skipped, 实际 skipped={skipped}, other={other}");
    assert_eq!(other, 0, "非 skipped 应为 0, 实际 {other}");
}

