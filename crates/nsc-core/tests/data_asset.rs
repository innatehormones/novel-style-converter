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
    assert!(got.locked_at.is_none());
}

#[test]
fn lock_state_blocks_reparse() {
    let db = Db::open_in_memory().unwrap();
    let uid = seed_upload(&db);
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "t".into() }).unwrap();
    assert!(!db.data_assets().is_locked(da_id).unwrap());
    db.data_assets().set_locked(da_id).unwrap();
    assert!(db.data_assets().is_locked(da_id).unwrap());
    assert!(db.data_assets().delete_if_unlocked(da_id).is_err());
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
    // 删除未锁定的 data_asset → chapters 关联行应被 FK CASCADE 清掉。
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
    db.data_assets().delete_if_unlocked(da_id).unwrap();
    assert_eq!(db.chapters().list_by_data_asset(da_id).unwrap().len(), 0);
    assert!(db.data_assets().get(da_id).unwrap().is_none());
}

#[test]
fn delete_data_asset_nonexistent_returns_error() {
    let db = Db::open_in_memory().unwrap();
    let err = db.data_assets().delete_if_unlocked(999).unwrap_err();
    assert!(err.to_string().contains("不存在"));
}