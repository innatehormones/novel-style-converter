//! Integration tests: JobQueue worker honors JobSpec.model_config.base_url.
//! See docs/superpowers/specs/2026-07-21-gpui-component-stage5-transform-dialog-design.md §6.2.

use std::time::Duration;

use nsc_core::ai::OpenAiProvider;
use nsc_core::db::Db;
use nsc_core::models::{
    Chapter, NewChapter, NewDataAsset, NewModelConfig, NewTransformationChapter,
    NewTransformationNovel, NewUpload, PromptKind, TransformMode, TransformStatus,
};
use nsc_core::transformer::{JobQueue, JobSpec};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// 轮询 queue snapshot 直到目标 transformation_id 出现在 done 或 failed 里。
/// timeout 内未出现则返回 false。
async fn wait_done(queue: &JobQueue, transformation_id: i64, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        let snap = queue.snapshot();
        if snap.done.iter().any(|j| j.transformation_id == transformation_id)
            || snap.failed.iter().any(|j| j.transformation_id == transformation_id)
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

/// 在临时目录里建一个 SQLite 文件,seed 内置 prompt,创建 upload /
/// transformation_novel / chapter / transformation_chapter,关闭 Db 后把
/// 路径与实体返回给测试主体。
///
/// 不能用 `Db::open_in_memory`:每个 worker 会通过 factory 重新打开,
/// 而内存数据库是 per-Connection 的,worker 看不到测试插入的数据。
fn seed_db(path: &std::path::Path) -> (Chapter, nsc_core::models::Prompt, i64, i64) {
    let db = Db::open(path).expect("open db");
    db.seed_builtin_prompts().expect("seed builtin prompts");

    // 找一个 Compress builtin prompt,作为本测试要用的 prompt。
    let prompt = db
        .prompts()
        .list()
        .expect("list prompts")
        .into_iter()
        .find(|p| p.kind == PromptKind::Compress)
        .expect("find compress prompt");

    let upload_id = db
        .uploads()
        .insert(&NewUpload {
            sha256: "h".into(),
            filename: "x.txt".into(),
            byte_size: 0,
            file_path: "/tmp/x.txt".into(),
            original_text: "Hello world".into(),
        })
        .expect("insert upload");

    let da_id = db
        .data_assets()
        .insert(&NewDataAsset { upload_id, title: "DA".into() })
        .expect("insert data_asset");

    let tn_id = db
        .transformation_novels()
        .insert(&NewTransformationNovel {
            data_asset_id: da_id,
            title: "Test".into(),
        })
        .expect("insert transformation_novel");

    let chapter_id = db
        .chapters()
        .insert(&NewChapter {
            data_asset_id: da_id,
            idx: 1,
            title: "ch1".into(),
            byte_start: 0,
            byte_end: 11,
            word_count: 10,
        })
        .expect("insert chapter");

    let tx_id = db
        .transformation_chapters()
        .insert(&NewTransformationChapter {
            transformation_novel_id: tn_id,
            chapter_id,
            mode: TransformMode::Compress,
            prompt_id: prompt.id,
            model_config_id: 0, // 测试 1 / 2 / 3 各自覆盖
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
        })
        .expect("insert transformation_chapter");

    let chapter = db
        .chapters()
        .get(chapter_id)
        .expect("get chapter")
        .expect("chapter exists");
    drop(db);
    (chapter, prompt, tx_id, da_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_hits_url_from_model_config_base_url() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (chapter, prompt, _initial_tx_id, da_id) = seed_db(&db_path);

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-1",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "compressed"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        })))
        .mount(&mock)
        .await;

    // 把 cfg 插进 DB,得到 id,再读回完整 cfg(带 server 分配的 id)。
    let cfg_id = {
        let db = Db::open(&db_path).unwrap();
        db.model_configs()
            .insert(&NewModelConfig {
                name: "mock-cfg".into(),
                base_url: mock.uri(),
                api_key: "k".into(),
                model: "m".into(),
                max_tokens: None,
                temperature: None,
                concurrency: 1,
            })
            .unwrap()
    };
    let cfg = {
        let db = Db::open(&db_path).unwrap();
        db.model_configs().get(cfg_id).unwrap().expect("cfg exists")
    };

    // 建一条 tx 指向真实 cfg id。
    let tx_id = {
        let db = Db::open(&db_path).unwrap();
        let prompt_id = prompt.id;
        db.transformation_chapters()
            .insert(&NewTransformationChapter {
                transformation_novel_id: {
                    // 复用 seed 创建的 tn;通过 tn.list() 找出指向同 upload 的那个。
                    db.transformation_novels()
                        .list_by_data_asset(da_id)
                        .unwrap()
                        .into_iter()
                        .next()
                        .unwrap()
                        .id
                },
                chapter_id: chapter.id,
                mode: TransformMode::Compress,
                prompt_id,
                model_config_id: cfg.id,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
            })
            .unwrap()
    };

    let path_for_factory = db_path.clone();
    let queue = JobQueue::new(
        1,
        move || Db::open(&path_for_factory),
        |cfg: &nsc_core::models::ModelConfig| {
            Box::new(
                OpenAiProvider::new(cfg.base_url.clone(), cfg.api_key.clone())
                    .expect("provider"),
            )
        },
    );
    queue.enqueue(JobSpec {
        transformation_id: tx_id,
        mode: TransformMode::Compress,
        chapter: chapter.clone(),
        prompt: prompt.clone(),
        model_config: cfg.clone(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    });

    assert!(
        wait_done(&queue, tx_id, Duration::from_secs(10)).await,
        "worker should finish within 10s"
    );

    let db = Db::open(&db_path).unwrap();
    let tx = db
        .transformation_chapters()
        .get(tx_id)
        .unwrap()
        .expect("tx exists");
    assert_eq!(tx.status, TransformStatus::Done);
    assert!(tx.tokens_in.is_some(), "tokens_in should be recorded");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_marks_failed_on_http_401() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (chapter, prompt, _initial_tx_id, da_id) = seed_db(&db_path);

    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock)
        .await;

    let cfg_id = {
        let db = Db::open(&db_path).unwrap();
        db.model_configs()
            .insert(&NewModelConfig {
                name: "x".into(),
                base_url: mock.uri(),
                api_key: "k".into(),
                model: "m".into(),
                max_tokens: None,
                temperature: None,
                concurrency: 1,
            })
            .unwrap()
    };
    let cfg = {
        let db = Db::open(&db_path).unwrap();
        db.model_configs().get(cfg_id).unwrap().expect("cfg exists")
    };

    let tx_id = {
        let db = Db::open(&db_path).unwrap();
        let prompt_id = prompt.id;
        db.transformation_chapters()
            .insert(&NewTransformationChapter {
                transformation_novel_id: {
                    db.transformation_novels()
                        .list_by_data_asset(da_id)
                        .unwrap()
                        .into_iter()
                        .next()
                        .unwrap()
                        .id
                },
                chapter_id: chapter.id,
                mode: TransformMode::Compress,
                prompt_id,
                model_config_id: cfg.id,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
            })
            .unwrap()
    };

    let path_for_factory = db_path.clone();
    let queue = JobQueue::new(
        1,
        move || Db::open(&path_for_factory),
        |cfg: &nsc_core::models::ModelConfig| {
            Box::new(
                OpenAiProvider::new(cfg.base_url.clone(), cfg.api_key.clone())
                    .expect("provider"),
            )
        },
    );
    queue.enqueue(JobSpec {
        transformation_id: tx_id,
        mode: TransformMode::Compress,
        chapter: chapter.clone(),
        prompt: prompt.clone(),
        model_config: cfg.clone(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    });

    assert!(wait_done(&queue, tx_id, Duration::from_secs(10)).await);
    let snap = queue.snapshot();
    assert_eq!(snap.failed.len(), 1, "snapshot.failed should have 1 entry");
    assert_eq!(snap.failed[0].transformation_id, tx_id);

    let db = Db::open(&db_path).unwrap();
    let tx = db
        .transformation_chapters()
        .get(tx_id)
        .unwrap()
        .expect("tx exists");
    assert_eq!(tx.status, TransformStatus::Failed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_factory_receives_correct_base_url_per_job() {
    // 两个 mock server;入队两条 transformation,各自指向不同 base_url;
    // 每个 server 记录自己的请求;验证每个 worker 命中自己的 URL。
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let (chapter, prompt, _initial_tx_id, da_id) = seed_db(&db_path);

    let server_a = MockServer::start().await;
    let server_b = MockServer::start().await;
    let ok_response = ResponseTemplate::new(200).set_body_json(serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "ok"}}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }));
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_response.clone())
        .mount(&server_a)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ok_response)
        .mount(&server_b)
        .await;

    let (cfg_a_id, cfg_b_id) = {
        let db = Db::open(&db_path).unwrap();
        let a = db
            .model_configs()
            .insert(&NewModelConfig {
                name: "A".into(),
                base_url: server_a.uri(),
                api_key: "ka".into(),
                model: "m".into(),
                max_tokens: None,
                temperature: None,
                concurrency: 1,
            })
            .unwrap();
        let b = db
            .model_configs()
            .insert(&NewModelConfig {
                name: "B".into(),
                base_url: server_b.uri(),
                api_key: "kb".into(),
                model: "m".into(),
                max_tokens: None,
                temperature: None,
                concurrency: 1,
            })
            .unwrap();
        (a, b)
    };
    let (cfg_a, cfg_b) = {
        let db = Db::open(&db_path).unwrap();
        let a = db.model_configs().get(cfg_a_id).unwrap().expect("cfg a");
        let b = db.model_configs().get(cfg_b_id).unwrap().expect("cfg b");
        (a, b)
    };

    let (tx_a, tx_b) = {
        let db = Db::open(&db_path).unwrap();
        let tn_id = db
            .transformation_novels()
            .list_by_data_asset(da_id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .id;
        let prompt_id = prompt.id;
        let ta = db
            .transformation_chapters()
            .insert(&NewTransformationChapter {
                transformation_novel_id: tn_id,
                chapter_id: chapter.id,
                mode: TransformMode::Compress,
                prompt_id,
                model_config_id: cfg_a.id,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
            })
            .unwrap();
        let tb = db
            .transformation_chapters()
            .insert(&NewTransformationChapter {
                transformation_novel_id: tn_id,
                chapter_id: chapter.id,
                mode: TransformMode::Compress,
                prompt_id,
                model_config_id: cfg_b.id,
                ctx_prev_original: 0,
                ctx_prev_transformed: 0,
                ctx_next_original: 0,
            })
            .unwrap();
        (ta, tb)
    };

    let path_for_factory = db_path.clone();
    let queue = JobQueue::new(
        2,
        move || Db::open(&path_for_factory),
        |cfg: &nsc_core::models::ModelConfig| {
            Box::new(
                OpenAiProvider::new(cfg.base_url.clone(), cfg.api_key.clone())
                    .expect("provider"),
            )
        },
    );
    queue.enqueue(JobSpec {
        transformation_id: tx_a,
        mode: TransformMode::Compress,
        chapter: chapter.clone(),
        prompt: prompt.clone(),
        model_config: cfg_a.clone(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    });
    queue.enqueue(JobSpec {
        transformation_id: tx_b,
        mode: TransformMode::Compress,
        chapter: chapter.clone(),
        prompt: prompt.clone(),
        model_config: cfg_b.clone(),
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    });

    assert!(wait_done(&queue, tx_a, Duration::from_secs(10)).await);
    assert!(wait_done(&queue, tx_b, Duration::from_secs(10)).await);

    let reqs_a = server_a.received_requests().await.unwrap();
    let reqs_b = server_b.received_requests().await.unwrap();

    // 每个 server 应恰好收到 1 条请求 —— 证明没有交叉污染。
    // 若 worker 用了硬编码 URL,两个 server 会各收到 2 条,或某个 server 收到 0 条,
    // 下面的断言两种回归都能抓到。
    assert_eq!(reqs_a.len(), 1, "server A should have received exactly 1 request");
    assert_eq!(reqs_b.len(), 1, "server B should have received exactly 1 request");

    // 附加校验:每条请求都打在预期的 endpoint 上。
    // 注意:wiremock 会用 Host 头(localhost,无 port)重建 `url`,
    // 无法据此区分两台 server —— 真正证明路由的是上面「各恰好 1 条」的计数。
    assert_eq!(reqs_a[0].url.path(), "/chat/completions");
    assert_eq!(reqs_b[0].url.path(), "/chat/completions");
}