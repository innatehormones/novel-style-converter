use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::models::{
    ModelConfig, NewChapter, NewTransformationChapter, NewTransformationNovel, NewUpload, Prompt,
    PromptKind, TransformMode, TransformStatus,
};
use nsc_core::transformer::{JobQueue, JobSpec};
use tempfile::TempDir;

/// 始终成功的 provider,返回固定 (content, tokens_in, tokens_out)。
struct OkProvider;
#[async_trait]
impl AiProvider for OkProvider {
    async fn chat(&self, _req: ChatRequest) -> nsc_core::Result<ChatResponse> {
        Ok(ChatResponse { content: "ok".into(), tokens_in: 4, tokens_out: 6 })
    }
}

/// 始终失败的 provider。
struct FailProvider;
#[async_trait]
impl AiProvider for FailProvider {
    async fn chat(&self, _req: ChatRequest) -> nsc_core::Result<ChatResponse> {
        Err(nsc_core::Error::Ai("boom".into()))
    }
}

/// 在临时目录里建一个 SQLite 文件,seed 内置 prompt,创建一个 upload /
/// transformation_novel / chapter / transformation_chapter,关闭 Db 后把
/// `(TempDir, 路径, transformation_chapter_id)` 返回给测试主体。
///
/// 不能用 `Db::open_in_memory`:每个 worker 会通过 factory 重新打开,
/// 而内存数据库是 per-Connection 的,worker 看不到测试插入的数据。
fn setup() -> (TempDir, PathBuf, i64) {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("test.db");
    let db = Db::open(&path).unwrap();
    db.seed_builtin_prompts().unwrap();
    let upload_id = db.uploads().insert(&NewUpload {
        sha256: "h".into(),
        filename: "x.txt".into(),
        byte_size: 0,
        file_path: "/tmp/x.txt".into(),
        original_text: "abc".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&nsc_core::models::NewDataAsset {
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
        title: "a".into(),
        byte_start: 0,
        byte_end: 3,
        word_count: 3,
    }).unwrap();
    let tid = db.transformation_chapters().insert(&NewTransformationChapter {
        transformation_novel_id: tn_id,
        chapter_id: cid,
        mode: TransformMode::Compress,
        prompt_id: 1,
        model_config_id: 1,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        batch_id: None,
        style_ref_chapter_id: None,
    }).unwrap();
    drop(db);
    (dir, path, tid)
}

/// 用 `path` 打开 Db,组装一个 `JobSpec` 给 queue 用。
fn make_spec(path: &std::path::Path, tid: i64) -> JobSpec {
    let db = Db::open(path).unwrap();
    let t = db.transformation_chapters().get(tid).unwrap().unwrap();
    let chapter = db.chapters().get(t.chapter_id).unwrap().unwrap();
    drop(db);
    JobSpec {
        transformation_id: tid,
        mode: TransformMode::Compress,
        chapter,
        prompt: Prompt {
            id: 0,
            name: "p".into(),
            kind: PromptKind::Compress,
            template: "X={{chapter_content}}".into(),
            is_builtin: false,
        },
        model_config: ModelConfig {
            id: 0,
            name: "m".into(),
            base_url: "x".into(),
            api_key: "k".into(),
            model: "x".into(),
            max_tokens: None,
            temperature: None,
            concurrency: 1,
        },
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    }
}

/// 跑通成功路径:fake provider → snapshot 进入 done,DB 状态 = done。
/// spec §5.x 收敛后,worker 不再写 `tc.result_content`(正文走结果集槽,本测试
/// 不接 scheduler/result_slot,只能验证 tokens)。槽写入由
/// `BatchScheduler::on_chapter_done` 在 notifier 回调里完成,见
/// `tests/scheduler.rs::worker_success_writes_workflow_result_slot_not_tc_result_content`。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn runs_one_job_to_done_with_fake_provider() {
    let (_dir, path, tid) = setup();
    let path_for_factory = path.clone();
    let q = JobQueue::new(
        2,
        move || Db::open(&path_for_factory),
        |_cfg| Box::new(OkProvider),
    );
    q.enqueue(make_spec(&path, tid));

    // 最多等 5s 让 worker 把 job 跑完
    for _ in 0..100 {
        if q.snapshot().done.len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let snap = q.snapshot();
    assert_eq!(snap.done.len(), 1, "snapshot.done should have 1 entry");
    assert_eq!(snap.done[0].tokens_in, Some(4));
    assert_eq!(snap.done[0].tokens_out, Some(6));

    // DB 写库验证 —— tokens 写入,但 result_content 留 NULL(spec 收口到结果集)。
    let db = Db::open(&path).unwrap();
    let t = db.transformation_chapters().get(tid).unwrap().unwrap();
    assert_eq!(t.status, TransformStatus::Done);
    assert_eq!(t.tokens_in, Some(4));
    assert_eq!(t.tokens_out, Some(6));
    assert!(t.result_content.is_none(), "tc.result_content 不再写;走结果集槽");
}

/// 跑通失败路径:fake provider → snapshot 进入 failed,DB 状态 = failed
/// 且 error 字段写入。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failure_records_failed_snapshot_and_db_status() {
    let (_dir, path, tid) = setup();
    let path_for_factory = path.clone();
    let q = JobQueue::new(
        1,
        move || Db::open(&path_for_factory),
        |_cfg| Box::new(FailProvider),
    );
    q.enqueue(make_spec(&path, tid));

    for _ in 0..100 {
        if q.snapshot().failed.len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let snap = q.snapshot();
    assert_eq!(snap.failed.len(), 1, "snapshot.failed should have 1 entry");
    let err = snap.failed[0].error.clone().unwrap_or_default();
    assert!(err.contains("boom"), "error should mention boom, got: {err}");

    let db = Db::open(&path).unwrap();
    let t = db.transformation_chapters().get(tid).unwrap().unwrap();
    assert_eq!(t.status, TransformStatus::Failed);
    let db_err = t.error.unwrap_or_default();
    assert!(db_err.contains("boom"), "DB error should mention boom, got: {db_err}");
}