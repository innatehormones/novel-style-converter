use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::db::Db;
use nsc_core::models::{
    BatchStatus, NewChapter, NewDataAsset, NewModelConfig, NewTransformationNovel,
    NewUpload, OnFailurePolicy, PromptKind,
};
use nsc_core::transformer::{BatchScheduler, JobQueue, WorkflowCreate};

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

fn seed_data(path: &std::path::Path, n: usize) -> (i64, i64, Vec<i64>) {
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
        temperature: None,
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
    (tn_id, cfg_id, cids)
}

fn open_db(path: &std::path::Path) -> Result<Arc<Db>, nsc_core::error::Error> {
    Db::open(path)
}

#[test]
fn reproduce_busy_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let (tn_id, cfg_id, cids) = seed_data(&path, 10);

    let path_for_factory = path.clone();
    let queue = Arc::new(JobQueue::new(
        2,
        move || Db::open(&path_for_factory),
        |_cfg| -> Box<dyn AiProvider> { Box::new(SlowEchoProvider) },
        Arc::new(nsc_core::recorder::NoopRecorder),
    ));

    let shared_db = nsc_core::db::Db::open(&path).unwrap();
    let sched = Arc::new(BatchScheduler::new(
        shared_db.clone(),
        queue.clone(),
        Arc::new(|_cfg| -> Box<dyn AiProvider> { Box::new(SlowEchoProvider) }),
        Arc::new(nsc_core::recorder::NoopRecorder),
    ));
    {
        let sched_for_cb = sched.clone();
        queue.set_notifier(Arc::new(move |tid, success, error, content| {
            if !success && error.is_none() { return; }
            let res = if success {
                sched_for_cb.on_chapter_done(tid, content)
            } else {
                sched_for_cb.on_chapter_failed(tid, error.unwrap_or_default())
            };
            if let Err(e) = res {
                eprintln!("[notify] 处理失败: {e}");
            }
        }));
    }

    let batch = sched.create_workflow(WorkflowCreate {
        transformation_novel_id: tn_id,
        label: None,
        chapter_ids: cids.clone(),
        prompt_id: 1,
        model_config_id: cfg_id,
        mode: PromptKind::Compress,
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
        on_failure_policy: OnFailurePolicy::SkipFailed,
    }).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let final_status = loop {
        match open_db(&path) {
            Ok(db) => {
                let b = db.batches().get(batch.id).unwrap().unwrap();
                if !matches!(b.status, BatchStatus::Running) {
                    break b.status;
                }
            }
            Err(e) => {
                eprintln!("[test] open_db failed: {e}, retrying...");
            }
        }
        if Instant::now() > deadline {
            panic!("batch 未在 10s 内收尾, snapshot={:?}", queue.snapshot());
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    eprintln!("[test] batch final status: {:?}", final_status);
    let db = open_db(&path).unwrap();
    let mode: String = db.lock().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    eprintln!("[test] journal_mode = {}", mode);
    assert_eq!(final_status, BatchStatus::Stopped, "batch 必须收尾 Stopped, 实际 {final_status:?}");

    let db = open_db(&path).unwrap();
    for cid in &cids {
        let content: Option<String> = db.lock().query_row(
            "SELECT content FROM workflow_result_chapters WHERE chapter_id = ?1",
            rusqlite::params![cid],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(content, Some("ECHO_CONTENT".to_string()), "wrc.content for chapter {} 应该是 ECHO_CONTENT", cid);
    }
}
