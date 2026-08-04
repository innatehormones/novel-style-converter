use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use nsc_core::ai::{AiProvider, OpenAiProvider};
use nsc_core::db::Db;
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
    db.lock().expect("seed lock").seed_default_model_from_env().expect("seed default model from env");

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
    ));
    let scheduler = Arc::new(BatchScheduler::new(path.clone(), job_queue.clone()));
    {
        let sched = scheduler.clone();
        let notify: Notifier = Arc::new(move |tid, success, error| {
            let res = if success {
                sched.on_chapter_done(tid)
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
        .setup(|_app| Ok(()))
        .invoke_handler(tauri::generate_handler![
            commands::models::list_models,
            commands::models::upsert_model,
            commands::models::delete_model,
            commands::models::test_model,
            commands::uploads::list_uploads,
            commands::uploads::upload_file,
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
            commands::data_assets::get_data_asset_content,
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
            commands::prompts::count_transformation_chapters_by_prompt,
            commands::batches::list_batches,
            commands::batches::get_batch,
            commands::batches::create_batch,
            commands::batches::update_batch,
            commands::batches::list_batch_chapters,
            commands::batches::count_batches_by_status,
            commands::batches::resume_batch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
