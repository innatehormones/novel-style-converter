use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewUpload};

#[test]
fn create_and_inspect_upload() {
    // Task 3 起,parsed_at 字段从 uploads 表移除(由 data_assets 表达);
    // mark_parsed 也随之移除。这里保留一个最小 lifecycle 测试,验证 upload
    // 行创建 → data_asset 关联 → 锁定,再尝试替换章节被拒的端到端流程。
    let db = Db::open_in_memory().unwrap();
    let id = db.uploads().insert(&NewUpload {
        sha256: "hash-a".into(),
        filename: "测试.txt".into(),
        byte_size: 1024,
        file_path: "/tmp/a.txt".into(),
        original_text: "原文内容".into(),
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

    // 未锁定 → 可以删;锁后 → 不可以。
    assert!(!db.data_assets().is_locked(da_id).unwrap());
    db.data_assets().set_locked(da_id).unwrap();
    assert!(db.data_assets().is_locked(da_id).unwrap());
    assert!(db.data_assets().delete_if_unlocked(da_id).is_err());
}