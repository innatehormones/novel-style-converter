//! AI 调用 recorder —— 把每次 LLM chat 调用的元数据 / 预览落库,供 UI 看板 / 排查 / 计量用。
//!
//! ## 设计意图
//!
//! - **非阻塞**:`recorder.record(event)` 只把事件丢进 mpsc channel,立刻返回;
//!   真正的 `db.ai_call_logs().insert(...)` 由后台 task 批量落库。
//!   transformer / test_model 在 hot path,不能为记账等 DB。
//! - **fail-fast 不退化**:背景 task 落库失败时 `eprintln!` 报警但不阻塞业务 —— 业务调用
//!   本身已经成功(返回给用户的转换结果 / test_model 报告不受影响),只是少了一行日志。
//!   反过来:recorder 自身不能用 Result 阻塞 hot path,否则每个转换都要 await DB。
//! - **no-op 实现给测试用**:`NoopRecorder` 单元测试 / `open_in_memory` 不需要真落库时用。
//! - **channel 满处理**:容量由 `ChannelRecorder::new(cap)` 决定,满时 `try_send` 失败 → drop new
//!   (MPSC 不支持 pop,改成新事件覆盖最旧会破 channel 抽象;这里直接 drop new,
//!   简单稳;真要严格保序,future work 加 Arc<Mutex<VecDeque>> + condvar)。
//!
//! ## Phase 1 / Phase 2 边界
//!
//! - **本模块 Phase 1 已经可用**:`AiCallEvent` / `AiCallRecorder` trait /
//!   `NoopRecorder` / `ChannelRecorder` 全部就绪。
//! - **embed 在哪里调用** 是 Phase 2 的事:`DefaultTransformer::transform` 与
//!   `commands::models::test_model` 还没接 recorder,等下一轮。

use crate::db::Db;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use tokio::sync::mpsc;

use crate::error::Result;
use crate::models::{AiCallBusiness, AiCallStatus, NewAiCallLog};

/// 单次 AI 调用的 recorder 入参 —— 字段语义与 `NewAiCallLog` 一致;
/// recorder 内部负责调用 `truncate_preview` + 落库。
/// `created_at` 不在这里 —— 落库时由 repo 填当前 UTC,保证顺序与 wall-clock 同步。
#[derive(Debug, Clone)]
pub struct AiCallEvent {
    pub business: AiCallBusiness,
    pub context_type: Option<String>,
    pub context_id: Option<i64>,
    pub model_config_id: Option<i64>,
    pub model_name: String,
    pub base_url: String,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub system_full: String,           // 完整 system 消息,recorder 内部截 10KB
    pub user_full: String,             // 完整 user 消息
    pub estimated_tokens_in: Option<i32>,
    pub actual_tokens_in: Option<i32>,
    pub actual_tokens_out: Option<i32>,
    pub status: AiCallStatus,
    pub response_full: String,         // 完整 response,recorder 内部截 10KB
    pub latency_ms: i64,
    pub error: Option<String>,
}

/// Recorder 抽象 —— hot path 只调 `record(event)`,不 await。
/// 多线程共享 `Arc<dyn AiCallRecorder>`。
pub trait AiCallRecorder: Send + Sync {
    /// 不阻塞,丢事件。
    fn record(&self, event: AiCallEvent);
    /// 估算 pending 队列深度(供 UI 看板 / 监控用;no-op 实现返回 0)。
    fn pending(&self) -> usize;
}

/// 啥也不做的 recorder —— 单元测试 / `Db::open_in_memory` 场景用。
#[derive(Debug, Clone, Default)]
pub struct NoopRecorder;

impl AiCallRecorder for NoopRecorder {
    fn record(&self, _event: AiCallEvent) {}
    fn pending(&self) -> usize { 0 }
}

/// channel-backed recorder —— 把 event 丢进 mpsc,后台 task 落库。
/// `pending()` 返回 channel 当前队列长度(粗估)。
#[derive(Debug, Clone)]
pub struct ChannelRecorder {
    sender: mpsc::Sender<AiCallEvent>,
    pending: Arc<AtomicU64>,
}

impl ChannelRecorder {
    /// 创建一个 capacity = `cap` 的 channel recorder;background task 还没启,
    /// 调用方要拿到 handle 后再 spawn writer 接 DB。
    pub fn new(cap: usize) -> (Self, mpsc::Receiver<AiCallEvent>) {
        let (tx, rx) = mpsc::channel(cap);
        let pending = Arc::new(AtomicU64::new(0));
        (Self { sender: tx, pending }, rx)
    }

    /// 把当前 pending 计数自减 —— background task 落库后调,
    /// 让 sender 端也能精确表达"channel 里 + DB 落库中"的合并量。
    pub fn record_done(&self) {
        self.pending.fetch_sub(1, Ordering::Relaxed);
    }
}

impl AiCallRecorder for ChannelRecorder {
    fn record(&self, event: AiCallEvent) {
        // try_send:channel 满则丢。不阻塞 hot path。
        match self.sender.try_send(event) {
            Ok(()) => { self.pending.fetch_add(1, Ordering::Relaxed); }
            Err(mpsc::error::TrySendError::Full(_)) => {
                eprintln!("[recorder] channel full, dropping event");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // background task 死了,接受丢事件
            }
        }
    }
    fn pending(&self) -> usize {
        self.pending.load(Ordering::Relaxed) as usize
    }
}

/// 启动 background writer —— 监听 channel,逐条落库。
///
/// **不在调用方线程上 spawn tokio task**,而是自己 `std::thread::spawn` + 内置
/// `tokio::runtime::Builder::new_current_thread()`,跟 `JobQueue` worker 同一种
/// 解耦风格 —— 调用方是否在 tokio runtime 里、有没有 reactor,都不影响 recorder 工作。
/// 这点很重要:`src-tauri/lib.rs::run()` 是 builder 同步阶段,.run() 之前还没有 tokio
/// reactor,直接 `tokio::spawn` 会 panic `there is no reactor running`。
///
/// DB handle 是跨线程共享的 Arc<Db>(见 db::pool) —— main / worker / notifier 都持有同一份,
/// recorder writer 也复用同一份,不再按 path 重开连接,无 SQLITE_BUSY。
pub fn spawn_writer(
    db: Arc<Db>,
    recorder: ChannelRecorder,
    mut rx: mpsc::Receiver<AiCallEvent>,
) -> std::thread::JoinHandle<()> {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("[recorder] failed to build tokio runtime: {e}");
                return;
            }
        };
        rt.block_on(run_writer(db, &mut rx, &recorder));
    })
}

/// 真正的 background loop —— `spawn_writer` 包了 `tokio::spawn`,这里直接 `await`。
pub async fn run_writer(db: Arc<Db>, rx: &mut mpsc::Receiver<AiCallEvent>, recorder: &ChannelRecorder) {
    while let Some(event) = rx.recv().await {
        if let Err(e) = write_one(&db, &event).await {
            eprintln!("[recorder] insert failed: {e}");
        }
        recorder.record_done();
    }
}

async fn write_one(db: &Arc<Db>, event: &AiCallEvent) -> Result<()> {
    use crate::db::repo::truncate_preview;
    let (sys_prev, sys_size) = truncate_preview(&event.system_full);
    let (user_prev, user_size) = truncate_preview(&event.user_full);
    let (resp_prev, resp_size) = truncate_preview(&event.response_full);
    let new = NewAiCallLog {
        business: event.business,
        context_type: event.context_type.clone(),
        context_id: event.context_id,
        model_config_id: event.model_config_id,
        model_name: event.model_name.clone(),
        base_url: event.base_url.clone(),
        temperature: event.temperature,
        max_tokens: event.max_tokens,
        system_preview: sys_prev,
        user_preview: user_prev,
        system_size: sys_size,
        user_size,
        estimated_tokens_in: event.estimated_tokens_in,
        actual_tokens_in: event.actual_tokens_in,
        actual_tokens_out: event.actual_tokens_out,
        status: event.status,
        response_preview: resp_prev,
        response_size: resp_size,
        latency_ms: event.latency_ms,
        error: event.error.clone(),
    };
    db.ai_call_logs().insert(&new)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AiCallBusiness;

    fn dummy_event() -> AiCallEvent {
        AiCallEvent {
            business: AiCallBusiness::TestModel,
            context_type: None,
            context_id: None,
            model_config_id: Some(1),
            model_name: "gpt-4o-mini".into(),
            base_url: "https://api.example.com/v1".into(),
            temperature: Some(0.7),
            max_tokens: Some(128),
            system_full: "you are a translator".into(),
            user_full: "hello".into(),
            estimated_tokens_in: Some(3),
            actual_tokens_in: Some(5),
            actual_tokens_out: Some(8),
            status: AiCallStatus::Success,
            response_full: "hi there".into(),
            latency_ms: 250,
            error: None,
        }
    }

    #[test]
    fn noop_recorder_does_nothing() {
        let r = NoopRecorder;
        r.record(dummy_event());
        assert_eq!(r.pending(), 0);
    }

    #[tokio::test]
    async fn channel_recorder_passes_events_through() {
        let (rec, mut rx) = ChannelRecorder::new(8);
        rec.record(dummy_event());
        rec.record(dummy_event());
        assert_eq!(rec.pending(), 2);
        let e1 = rx.recv().await.unwrap();
        let e2 = rx.recv().await.unwrap();
        assert_eq!(e1.model_name, "gpt-4o-mini");
        assert_eq!(e2.business, AiCallBusiness::TestModel);
        rec.record_done();
        rec.record_done();
        assert_eq!(rec.pending(), 0);
    }

    #[tokio::test]
    async fn channel_full_drops_event_no_block() {
        let (rec, _rx) = ChannelRecorder::new(1);
        rec.record(dummy_event());
        // 第二条会因 channel 满被 try_send 拒绝,eprintln 但不阻塞、不 panic
        rec.record(dummy_event());
        assert_eq!(rec.pending(), 1);
    }
}