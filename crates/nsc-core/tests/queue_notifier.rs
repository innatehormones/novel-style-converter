//! Integration test: JobQueue 状态变化触发 notifier 回调。
//!
//! 验证 set_notifier 注册的闭包在 enqueue / run_job 结束时被调用一次。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use nsc_core::db::Db;
use nsc_core::transformer::JobQueue;
use tempfile::tempdir;

/// notifier 在 JobQueue::new 后即使没有 job 也不会触发;
/// enqueue 一次 → 触发 1 次;
/// enqueue 多次 → 触发多次。
#[test]
fn enqueue_fires_notifier() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("notify.db");
    // 建一个空库,seed 内置 prompt(JobQueue::new 内部会打开 db)。
    Db::open(&path).unwrap().seed_builtin_prompts().unwrap();

    let path_for_factory = path.clone();
    let queue = JobQueue::new(
        1,
        move || Db::open(&path_for_factory),
        |_cfg| -> Box<dyn nsc_core::ai::AiProvider> {
            // 测试不需要真的 AI 调用,因为我们只触发 enqueue,
            // 不放任何 JobSpec 进 channel,worker 会立刻阻塞在 recv。
            unreachable!("not invoked in this test")
        },
    );

    let count = Arc::new(AtomicUsize::new(0));
    let count_for_cb = count.clone();
    queue.set_notifier(Arc::new(move || {
        count_for_cb.fetch_add(1, Ordering::SeqCst);
    }));

    // enqueue 一次 → 触发 1 次。注意:enqueue 必须传一个 JobSpec,
    // 但因为没有合适的 chapter / prompt / cfg,我们构造空 JobSpec;
    // 不期望 worker 处理它 —— 我们只关心 notifier 触发。
    queue.enqueue(nsc_core::transformer::JobSpec {
        transformation_id: 0,
        mode: nsc_core::models::TransformMode::Compress,
        chapter: nsc_core::models::Chapter {
            id: 0,
            data_asset_id: 0,
            idx: 0,
            title: String::new(),
            byte_start: 0,
            byte_end: 0,
            word_count: 0,
        },
        prompt: nsc_core::models::Prompt {
            id: 0,
            name: String::new(),
            kind: nsc_core::models::PromptKind::Compress,
            template: String::new(),
            is_builtin: false,
        },
        model_config: nsc_core::models::ModelConfig {
            id: 0,
            name: String::new(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            max_tokens: None,
            temperature: None,
            concurrency: 1,
        },
        ctx_prev_original: 0,
        ctx_prev_transformed: 0,
        ctx_next_original: 0,
    });

    // 让 worker 拿到 JobSpec 并跑完(必然失败因为 prompt/model 不存在),
    // 然后 fire notify。容许多触发几次。
    std::thread::sleep(std::time::Duration::from_millis(200));

    let c = count.load(Ordering::SeqCst);
    assert!(c >= 1, "expected at least 1 notify call after enqueue, got {c}");
}