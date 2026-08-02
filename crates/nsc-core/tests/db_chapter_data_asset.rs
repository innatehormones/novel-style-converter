use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewUpload};

#[test]
fn chapter_belongs_to_data_asset_not_upload() {
    let db = Db::open_in_memory().unwrap();
    let uid = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "n.txt".into(),
        byte_size: 10, file_path: "/p".into(),
        original_text: "第一章 起\n一段文字\n第二章 出\n二段内容".into(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "n".into() }).unwrap();
    db.chapters().insert_batch(&[
        nsc_core::models::NewChapter { data_asset_id: da_id, idx: 1, title: "第一章".into(), byte_start: 0, byte_end: 10, word_count: 4 },
        nsc_core::models::NewChapter { data_asset_id: da_id, idx: 2, title: "第二章".into(), byte_start: 11, byte_end: 22, word_count: 4 },
    ]).unwrap();
    let chs = db.chapters().list_by_data_asset(da_id).unwrap();
    assert_eq!(chs.len(), 2);
    assert_eq!(chs[0].title, "第一章");
    assert_eq!(chs[1].data_asset_id, da_id);
}