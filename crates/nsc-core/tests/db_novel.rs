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