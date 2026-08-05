use nsc_core::db::Db;
use nsc_core::models::{NewChapter, NewDataAsset, NewUpload};

fn seed_upload(db: &Db) -> i64 {
    db.uploads().insert(&NewUpload {
        sha256: "abc".into(), filename: "x.txt".into(),
        byte_size: 100, file_path: "/tmp/x.txt".into(),
        original_text: "x".into(),
        word_count: 0,
    }).unwrap()
}

#[test]
fn insert_get_data_asset() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = seed_upload(&db);
    let id = db.data_assets().insert(&NewDataAsset {
        upload_id, title: "test".into(),
    }).unwrap();
    let got = db.data_assets().get(id).unwrap().unwrap();
    assert_eq!(got.upload_id, upload_id);
    assert_eq!(got.title, "test");
    assert!(got.parsed_at.timestamp() > 0);
}

#[test]
fn delete_data_asset_cascades_chapters_and_tns() {
    // 删 data_asset → FK CASCADE 把 chapters + transformation_novels +
    // transformation_chapters 一起带走(migration 0004/0005/0006 + 0002 + 0012)。
    let db = Db::open_in_memory().unwrap();
    let uid = seed_upload(&db);
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "t".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&nsc_core::models::NewTransformationNovel {
        data_asset_id: da_id, title: "tn".into(),
        default_model_config_id: None, default_prompt_id: None, default_mode: None,
    }).unwrap();
    db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 0, title: "ch1".into(),
        byte_start: 0, byte_end: 5, word_count: 1,
    }).unwrap();
    assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 1);
    assert_eq!(db.transformation_novels().list_by_data_asset(da_id).unwrap().len(), 1);
    db.data_assets().delete(da_id).unwrap();
    assert!(db.data_assets().get(da_id).unwrap().is_none());
    assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 0);
    assert_eq!(db.transformation_novels().list_by_data_asset(da_id).unwrap().len(), 0);
    assert!(db.transformation_novels().get(tn_id).unwrap().is_none());
}

#[test]
fn unique_upload_id() {
    let db = Db::open_in_memory().unwrap();
    let uid = seed_upload(&db);
    db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "a".into() }).unwrap();
    assert!(db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "b".into() }).is_err());
}

#[test]
fn cascade_delete_on_upload() {
    let db = Db::open_in_memory().unwrap();
    let uid = seed_upload(&db);
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "x".into() }).unwrap();
    db.uploads().delete(uid).unwrap();
    assert!(db.data_assets().get(da_id).unwrap().is_none());
}

#[test]
fn delete_data_asset_cascades_to_chapters() {
    // 删 data_asset → FK CASCADE 把 chapters 关联行清掉(migration 0005)。
    let db = Db::open_in_memory().unwrap();
    let uid = seed_upload(&db);
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "t".into() }).unwrap();
    db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 0, title: "ch1".into(),
        byte_start: 0, byte_end: 10, word_count: 5,
    }).unwrap();
    db.chapters().insert(&NewChapter {
        data_asset_id: da_id, idx: 1, title: "ch2".into(),
        byte_start: 10, byte_end: 20, word_count: 5,
    }).unwrap();
    assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 2);
    db.data_assets().delete(da_id).unwrap();
    assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 0);
    assert!(db.data_assets().get(da_id).unwrap().is_none());
}

#[test]
fn delete_data_asset_nonexistent_returns_error() {
    let db = Db::open_in_memory().unwrap();
    let err = db.data_assets().delete(999).unwrap_err();
    assert!(err.to_string().contains("不存在"));
}