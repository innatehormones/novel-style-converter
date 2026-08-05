use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewUpload};

#[test]
fn create_and_inspect_upload() {
    // upload 行创建 → data_asset 关联 → 删 upload/data_asset 端到端流程。
    let db = Db::open_in_memory().unwrap();
    let id = db.uploads().insert(&NewUpload {
        sha256: "hash-a".into(),
        filename: "测试.txt".into(),
        byte_size: 1024,
        file_path: "/tmp/a.txt".into(),
        original_text: "原文内容".into(),
        word_count: 0,
    }).unwrap();
    assert!(id > 0);

    let u = db.uploads().get(id).unwrap().unwrap();
    assert_eq!(u.original_text, "原文内容");
    assert!(u.file_path.ends_with("a.txt"));

    // data_asset 替代了 parsed_at 语义。
    let da_id = db.data_assets().insert(&NewDataAsset {
        upload_id: id, title: "测试.txt".into(),
    }).unwrap();
    assert!(db.data_assets().get(da_id).unwrap().is_some());

    // 没有 business lock —— upload / data_asset 都能直接删,FK CASCADE 接住。
    db.data_assets().delete(da_id).unwrap();
    db.uploads().delete(id).unwrap();
    assert!(db.data_assets().get(da_id).unwrap().is_none());
    assert!(db.uploads().get(id).unwrap().is_none());
}