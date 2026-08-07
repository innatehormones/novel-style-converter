use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use nsc_core::ai::{AiProvider, OpenAiProvider};
use nsc_core::db::Db;
use nsc_core::recorder::{spawn_writer, AiCallRecorder, ChannelRecorder};
use nsc_core::transformer::{BatchScheduler, JobQueue, Notifier};

mod commands;

fn data_db_path() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("data.db")
}

pub fn run() {
    let _ = dotenvy::dotenv();

    let path = data_db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let db = Arc::new(Mutex::new(Db::open(&path).expect("failed to open db")));
    nsc_core::startup_recovery::run(&db.lock().expect("recovery lock").conn)
        .expect("startup safe-recovery failed");

    // ── AI 调用 recorder ────────────────────────────────────────────
    // Channel 容量 4096;满时 drop new(不阻塞 hot path)。writer 任务按 db_path 重开
    // DB 落库,worker 线程不持 DB 句柄,避免跨线程 Send/Sync 摩擦。
    // handle 留着 —— app 退出时 tokio runtime 自然结束 writer(通道 drop → recv → break);
    // 不主动 abort,让最后几行日志能落库。
    let (recorder, rx) = ChannelRecorder::new(4096);
    let _writer_handle = spawn_writer(path.clone(), recorder.clone(), rx);
    let recorder: Arc<dyn AiCallRecorder> = Arc::new(recorder);


    let db_path_for_workers = path.clone();
    let job_queue = Arc::new(JobQueue::new(
        2,
        move || Ok(Db::open(&db_path_for_workers).expect("worker db open")),
        |cfg: &nsc_core::models::ModelConfig| -> Box<dyn AiProvider> {
            Box::new(
                OpenAiProvider::new(cfg.base_url.clone(), cfg.api_key.clone())
                    .unwrap_or_else(|_| {
                        OpenAiProvider::new(cfg.base_url.clone(), String::new())
                            .expect("fallback openai")
                    }),
            )
        },
        recorder.clone(),
    ));
    let scheduler = Arc::new(BatchScheduler::new(path.clone(), job_queue.clone()));
    {
        let sched = scheduler.clone();
        let notify: Notifier = Arc::new(move |tid, success, error, content| {
            if !success && error.is_none() { return; }
            let res = if success {
                sched.on_chapter_done(tid, content)
            } else {
                sched.on_chapter_failed(tid, error.unwrap_or_default())
            };
            if let Err(e) = res {
                eprintln!("[BatchScheduler] notify 处理失败: {e}");
            }
        });
        job_queue.set_notifier(notify);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .manage(job_queue)
        .manage(scheduler)
        .manage(recorder)
        .setup(|_app| Ok(()))
        .invoke_handler(tauri::generate_handler![
            commands::models::list_models,
            commands::models::list_models_including_archived,
            commands::models::upsert_model,
            commands::models::delete_model,
            commands::models::restore_model,
            commands::models::test_model,
            commands::uploads::list_uploads,
            commands::uploads::upload_file,
            commands::uploads::preview_upload_deletion,
            commands::uploads::delete_upload,
            commands::uploads::get_upload,
            commands::uploads::get_upload_text,
            commands::uploads::update_upload_text,
            commands::chapters::list_chapter_segments,
            commands::chapters::list_committed_segments,
            commands::chapters::list_chapters,
            commands::chapters::get_chapter_contents,
            commands::chapters::get_chapter,
            commands::chapters::parse_chapters,
            commands::cleaning::preview_cleaning,
            commands::data_assets::list_data_asset_chapters,
            commands::data_assets::commit_data_asset,
            commands::data_assets::list_data_assets,
            commands::data_assets::find_data_asset_by_upload,
            commands::data_assets::delete_data_asset,
            commands::transformation_novels::list_transformation_novels,
            commands::transformation_novels::create_transformation_novel,
            commands::transformation_novels::update_transformation_novel,
            commands::transformation_novels::delete_transformation_novel,
            commands::transformations::list_transformation_chapters,
            commands::transformations::list_transformation_chapters_for_chapter,
            commands::transformations::enqueue_transformation_chapters,
            commands::transformations::enqueue_all_chapters,
            commands::transformations::get_queue_snapshot,
            commands::prompts::list_prompts,
            commands::prompts::get_prompt,
            commands::prompts::upsert_prompt,
            commands::prompts::delete_prompt,
            commands::prompts::restore_prompt,
            commands::prompts::list_prompts_including_archived,
            commands::prompts::count_prompt_usage,
            commands::workflows::list_transformation_source_chapters,
            commands::workflows::create_workflow,
            commands::workflows::list_workflows,
            commands::workflows::get_workflow,
            commands::workflows::list_workflow_chapters,
            commands::workflows::stop_workflow,
            commands::workflows::retry_workflow_chapters,
            commands::ai_call_logs::list_ai_call_logs,
            commands::ai_call_logs::get_ai_call_log,
            commands::ai_call_logs::clear_ai_call_logs,
            commands::ai_call_logs::list_ai_call_logs_by_context,
            commands::workflows::list_chapter_workflow_results,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
