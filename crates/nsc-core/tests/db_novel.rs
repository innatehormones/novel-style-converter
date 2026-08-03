use nsc_core::db::Db;
use nsc_core::models::NewUpload;

#[test]
fn create_list_delete_upload() {
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

    let listed = db.uploads().list().unwrap();
    assert_eq!(listed.len(), 1);
    let u = &listed[0];
    assert_eq!(u.id, id);
    assert_eq!(u.filename, "测试.txt");
    assert_eq!(u.byte_size, 1024);
    assert_eq!(u.sha256, "hash-a");
    assert_eq!(u.file_path, "/tmp/a.txt");
    assert_eq!(u.original_text, "原文内容");

    db.uploads().delete(id).unwrap();
    assert!(db.uploads().list().unwrap().is_empty());
}

/// Migration 0007 给老 upload 行填了默认值 0;`backfill_word_count` 用已存的
/// original_text 重算这些行。新上传走 upload.rs:75 一次算对,不需要回填。
#[test]
fn backfill_fills_legacy_zero_word_count() {
    let db = Db::open_in_memory().unwrap();

    // 模拟"老行":word_count 留 0,original_text 有真实文本。
    let legacy = db.uploads().insert(&NewUpload {
        sha256: "legacy".into(),
        filename: "老.txt".into(),
        byte_size: 100,
        file_path: "/tmp/legacy.txt".into(),
        original_text: "正文一万字".into(),
        word_count: 0,
    }).unwrap();

    // 新行:word_count 已正确。
    let fresh = db.uploads().insert(&NewUpload {
        sha256: "fresh".into(),
        filename: "新.txt".into(),
        byte_size: 100,
        file_path: "/tmp/fresh.txt".into(),
        original_text: "你好".into(),
        word_count: 2,
    }).unwrap();

    // 极老行:original_text 是空字符串(原文没存进 DB),保持 0。
    let empty = db.uploads().insert(&NewUpload {
        sha256: "empty".into(),
        filename: "空.txt".into(),
        byte_size: 100,
        file_path: "/tmp/empty.txt".into(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();

    let updated = db.uploads().backfill_word_count().unwrap();
    assert_eq!(updated, 1, "只有 legacy 行应当被回填");

    let rows = db.uploads().list().unwrap();
    let by_id = |id: i64| rows.iter().find(|u| u.id == id).unwrap().word_count;
    assert_eq!(by_id(legacy), 5, "正文一万字 = 5 个 alphanumeric");
    assert_eq!(by_id(fresh), 2, "已正确的行不应被改动");
    assert_eq!(by_id(empty), 0, "空 original_text 保持 0");

    // 幂等:再跑一次,没有需要改的行。
    assert_eq!(db.uploads().backfill_word_count().unwrap(), 0);
}