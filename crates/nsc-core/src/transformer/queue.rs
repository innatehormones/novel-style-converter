use std::result::Result as StdResult;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::ai::AiProvider;
use crate::db::Db;
use crate::error::Result;
use crate::models::{ModelConfig, TransformationChapter, TransformationNovel, TransformStatus};
use crate::transformer::{
    DefaultTransformer, JobInfo, JobSpec, JobStatus, QueueSnapshot,
    TransformRequest, Transformer,
};

use super::job::SharedQueue;

pub type DbFactory = Arc<dyn Fn() -> Result<Db> + Send + Sync>;
pub type ProviderFactory = Arc<dyn Fn(&ModelConfig) -> Box<dyn AiProvider> + Send + Sync>;
/// 队列状态变更回调。`(tid, success, error)`:
/// - `enqueue` → `(tid, false, None)`
/// - Done → `(tid, true, None)`
/// - Failed (含 prep 失败) → `(tid, false, Some(err))`
/// 闭包在 worker 线程上执行 —— 不要在闭包里做重活或再次阻塞。
pub type Notifier = Arc<dyn Fn(i64, bool, Option<String>) + Send + Sync>;

type NotifySlot = Arc<std::sync::Mutex<Option<Notifier>>>;

pub struct JobQueue {
    tx: mpsc::UnboundedSender<JobSpec>,
    shared: super::job::Shared,
    notify: NotifySlot,
}

impl JobQueue {
    /// 启动 `workers` 个 tokio current-thread worker,共享一个 mpsc 队列。
    ///
    /// **工厂闭包是 JobQueue 能跨线程工作的核心**(因为 `Db` 不是 `Sync`,
    /// `AiProvider` 不是 `Send` 共享的)。
    /// - `db_factory`:每个 worker 启动时调一次,拿到**独立 owned** `Db`。
    ///   典型实现:`move || Ok(Db::open(&db_path))`。
    /// - `provider_factory`:每个 job 调一次,基于 `ModelConfig` 生成 owned
    ///   `Box<dyn AiProvider>`。**必须返回 owned**(不能返回 `&'a dyn AiProvider`),
    ///   否则 `Box<dyn Transformer>` 装不下。
    ///
    /// 两个工厂都要求 `Send + Sync + 'static`(被 `Arc<dyn Fn ...>` 包了一层)。
    /// `workers < 1` 会 panic。
    pub fn new<F, P>(workers: usize, db_factory: F, provider_factory: P) -> Self
    where
        F: Fn() -> Result<Db> + Send + Sync + 'static,
        P: Fn(&ModelConfig) -> Box<dyn AiProvider> + Send + Sync + 'static,
    {
        assert!(workers >= 1, "at least 1 worker");
        let (tx, rx) = mpsc::unbounded_channel::<JobSpec>();
        let rx = Arc::new(Mutex::new(rx));
        let shared: super::job::Shared = Arc::new(SharedQueue::default());
        let db_factory: DbFactory = Arc::new(db_factory);
        let provider_factory: ProviderFactory = Arc::new(provider_factory);
        let notify: NotifySlot = Arc::new(std::sync::Mutex::new(None));

        for _ in 0..workers {
            let shared = shared.clone();
            let db_factory = db_factory.clone();
            let provider_factory = provider_factory.clone();
            let rx = rx.clone();
            let notify = notify.clone();
            std::thread::spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(_) => return,
                };
                rt.block_on(async move {
                    let mut db = match db_factory() {
                        Ok(d) => d,
                        Err(_) => return,
                    };
                    loop {
                        let job = {
                            let mut guard = rx.lock().await;
                            guard.recv().await
                        };
                        let Some(job) = job else { break };
                        let ai: Box<dyn AiProvider> = (provider_factory)(&job.model_config);
                        db = run_job(shared.clone(), db, ai, job, notify.clone()).await;
                    }
                });
            });
        }
        Self { tx, shared, notify }
    }

    /// 注册队列变更回调。每次 `enqueue` / job 状态转换(Running / Done / Failed)末尾触发。
    /// 闭包在 worker 线程上执行 —— 不要在闭包里做重活或再次阻塞。
    pub fn set_notifier(&self, notifier: Notifier) {
        *self.notify.lock().expect("notify lock") = Some(notifier);
    }

    fn fire(notify: &NotifySlot, tid: i64, success: bool, error: Option<String>) {
        if let Some(n) = notify.lock().expect("notify lock").as_ref() {
            n(tid, success, error);
        }
    }

    /// 入队一个 transform job。返回传入的 `JobSpec.transformation_id`(方便 caller 记录)。
    /// 内部通过 unbounded mpsc 派发给 worker;调用方需保证 `JobSpec` 字段齐全
    /// (job 字段由 `transformation_chapters` 行反查得到,通常在 command 层组装)。
    pub fn enqueue(&self, job: JobSpec) -> i64 {
        let id = job.transformation_id;
        self.tx.send(job).expect("queue alive");
        Self::fire(&self.notify, id, false, None);
        id
    }

    /// 拉当前队列快照(pending / running / done / failed 四组)。
    /// 内部锁争用时返回空 snapshot,不阻塞 caller —— 用于前端 UI 1s 轮询。
    pub fn snapshot(&self) -> QueueSnapshot {
        self.shared.inner.try_lock().map(|m| m.clone()).unwrap_or_default()
    }
}

struct Prep {
    transformation_novel: TransformationNovel,
    chapter: crate::models::Chapter,
    chapter_content: String,
    prev_orig: Vec<(String, String)>,
    prev_tx: Vec<TransformationChapter>,
    next_orig: Vec<(String, String)>,
}

struct Final {
    chapter_title: String,
    chapter_idx: i32,
    db_write: DbWrite,
}

enum DbWrite {
    Done { tokens_in: i32, tokens_out: i32 },
    Failed { err: String },
}

async fn run_job(
    shared: super::job::Shared,
    db: Db,
    ai: Box<dyn AiProvider>,
    job: JobSpec,
    notify: NotifySlot,
) -> Db {
    let tid = job.transformation_id;
    let chapter_title = job.chapter.title.clone();
    let chapter_idx = job.chapter.idx;

    let prep: StdResult<Prep, String> = read_context(&db, &job);
    let prep = match prep {
        Ok(p) => p,
        Err(err) => {
            let _ = db.transformation_chapters().mark_failed(tid, err.clone());
            push_failed(&shared, tid, String::new(), 0, err.clone()).await;
            JobQueue::fire(&notify, tid, false, Some(err));
            return db;
        }
    };

    let _ = db.transformation_chapters().mark_running(tid);

    let req = TransformRequest {
        chapter: prep.chapter,
        chapter_content: prep.chapter_content,
        novel_context: crate::transformer::TransformationNovelContext {
            transformation_novel: prep.transformation_novel,
            prev_original: prep.prev_orig,
            prev_transformed: prep.prev_tx,
            next_original: prep.next_orig,
        },
        prompt: job.prompt.clone(),
        model_config: job.model_config.clone(),
    };
    let tx: Box<dyn Transformer> = Box::new(DefaultTransformer { ai });
    let ai_result = tx.transform(req).await;

    let final_state: Final = apply_result(&db, tid, chapter_title, chapter_idx, ai_result);

    match final_state.db_write {
        DbWrite::Done { tokens_in, tokens_out, .. } => {
            push_running(
                &shared, tid,
                final_state.chapter_title.clone(),
                final_state.chapter_idx,
            ).await;
            push_done(
                &shared, tid,
                final_state.chapter_title,
                final_state.chapter_idx,
                tokens_in, tokens_out,
            ).await;
            JobQueue::fire(&notify, tid, true, None);
        }
        DbWrite::Failed { err } => {
            push_failed(
                &shared, tid,
                final_state.chapter_title,
                final_state.chapter_idx,
                err.clone(),
            ).await;
            JobQueue::fire(&notify, tid, false, Some(err));
        }
    }

    db
}

/// 同步读所有 job 上下文:从 uploads.original_text 切片 chapter / 邻章正文。
/// 通过 tid 反查 transformation_novel_id(避免 caller 多传字段)。
fn read_context(db: &Db, job: &JobSpec) -> StdResult<Prep, String> {
    let tid = job.transformation_id;
    let cid = job.chapter.id;
    let idx = job.chapter.idx;
    let data_asset_id = job.chapter.data_asset_id;

    let tx_chapter = db.transformation_chapters().get(tid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "transformation_chapter missing".to_string())?;
    let tn_id = tx_chapter.transformation_novel_id;

    let transformation_novel = db.transformation_novels().get(tn_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "transformation_novel missing".to_string())?;

    // 先反查 data_asset 拿 upload_id,再读 upload.original_text 作为切片坐标系。
    let data_asset = db.data_assets().get(data_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("data_asset {data_asset_id} 不存在"))?;
    let upload = db.uploads().get(data_asset.upload_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {} 不存在", data_asset.upload_id))?;
    let original = &upload.original_text;

    let chapter = db.chapters().get(cid)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "chapter missing".to_string())?;
    let chapter_content = slice_chapter(original, &chapter)
        .ok_or_else(|| format!("chapter {} 切片失败(超出原文长度)", chapter.id))?;

    let prev_chapters = db
        .chapters()
        .prev_n(data_asset_id, idx, job.ctx_prev_original)
        .unwrap_or_default();
    let next_chapters = db
        .chapters()
        .next_n(data_asset_id, idx, job.ctx_next_original)
        .unwrap_or_default();

    let prev_orig: Vec<(String, String)> = prev_chapters
        .into_iter()
        .map(|c| {
            let body = slice_chapter(original, &c).unwrap_or_default();
            (c.title, body)
        })
        .collect();
    let next_orig: Vec<(String, String)> = next_chapters
        .into_iter()
        .map(|c| {
            let body = slice_chapter(original, &c).unwrap_or_default();
            (c.title, body)
        })
        .collect();

    let prev_tx: Vec<TransformationChapter> = {
        let mut out = Vec::new();
        if let Ok(chs) = db.chapters().prev_n(data_asset_id, idx, 32) {
            let take = job.ctx_prev_transformed.max(0) as usize;
            for ch in chs.iter().take(take) {
                if let Ok(list) = db.transformation_chapters().list_by_chapter(ch.id) {
                    if let Some(t) = list.into_iter().find(|t| {
                        t.transformation_novel_id == tn_id
                            && t.prompt_id == job.prompt.id
                            && t.model_config_id == job.model_config.id
                            && matches!(t.status, TransformStatus::Done)
                    }) {
                        out.push(t);
                    }
                }
            }
        }
        out
    };

    Ok(Prep {
        transformation_novel,
        chapter,
        chapter_content,
        prev_orig,
        prev_tx,
        next_orig,
    })
}

/// 从 upload.original_text 切片出 chapter 内容。byte 范围非法或越界返回 None。
fn slice_chapter(text: &str, c: &crate::models::Chapter) -> Option<String> {
    let s = c.byte_start.max(0) as usize;
    let e = (c.byte_end.max(0) as usize).min(text.len());
    if s >= e || e > text.len() {
        return None;
    }
    if !text.is_char_boundary(s) || !text.is_char_boundary(e) {
        return None;
    }
    Some(text[s..e].to_string())
}

fn apply_result(
    db: &Db,
    tid: i64,
    chapter_title: String,
    chapter_idx: i32,
    ai_result: Result<crate::transformer::TransformOutcome>,
) -> Final {
    match ai_result {
        Ok(out) => {
            let _ = db.transformation_chapters().mark_done(
                tid, out.result_content, out.tokens_in, out.tokens_out,
            );
            Final {
                chapter_title, chapter_idx,
                db_write: DbWrite::Done {
                    tokens_in: out.tokens_in,
                    tokens_out: out.tokens_out,
                },
            }
        }
        Err(e) => {
            let err_str = e.to_string();
            let _ = db.transformation_chapters().mark_failed(tid, err_str.clone());
            Final {
                chapter_title, chapter_idx,
                db_write: DbWrite::Failed { err: err_str },
            }
        }
    }
}

async fn push_running(
    shared: &super::job::Shared,
    tid: i64,
    chapter_title: String,
    chapter_idx: i32,
) {
    let mut s = shared.inner.lock().await;
    s.running.push(JobInfo {
        transformation_id: tid,
        chapter_title, chapter_idx,
        status: JobStatus::Running,
        error: None, tokens_in: None, tokens_out: None,
    });
}

async fn push_done(
    shared: &super::job::Shared,
    tid: i64,
    chapter_title: String,
    chapter_idx: i32,
    tokens_in: i32, tokens_out: i32,
) {
    let mut s = shared.inner.lock().await;
    s.done.push(JobInfo {
        transformation_id: tid,
        chapter_title, chapter_idx,
        status: JobStatus::Done,
        error: None, tokens_in: Some(tokens_in), tokens_out: Some(tokens_out),
    });
}

async fn push_failed(
    shared: &super::job::Shared,
    tid: i64,
    chapter_title: String,
    chapter_idx: i32,
    err: String,
) {
    let mut s = shared.inner.lock().await;
    s.failed.push(JobInfo {
        transformation_id: tid,
        chapter_title, chapter_idx,
        status: JobStatus::Failed,
        error: Some(err),
        tokens_in: None, tokens_out: None,
    });
}