use nsc_core::db::Db;

#[test]
fn create_result_and_slots_then_list_chapters_returns_empty_slots() {
    let db = Db::open_in_memory().unwrap();
    // uploads → data_assets → transformation_novels → batches 串起来的最小 fixture。
    // data_assets.upload_id 与 chapters.data_asset_id 都指向 data_assets.id=1。
    db.conn.execute(
        "INSERT INTO uploads (sha256, filename, byte_size, uploaded_at, file_path, original_text, word_count) \
         VALUES ('sha-x', 'x.txt', 10, '2026-08-04T00:00:00+00:00', '/tmp/x.txt', '', 0)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO data_assets (upload_id, title, parsed_at) \
         VALUES (1, 'x', '2026-08-04T00:00:00+00:00')", [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO transformation_novels (title, data_asset_id, created_at) \
         VALUES ('tn', 1, '2026-08-04T00:00:00+00:00')", [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO chapters (data_asset_id, idx, title, byte_start, byte_end, word_count) \
         VALUES (1, 1, 'c1', 0, 10, 10), (1, 2, 'c2', 10, 20, 10), (1, 3, 'c3', 20, 30, 10)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
         VALUES (1, NULL, 'pause_and_review', 'running', '2026-08-04T00:00:00+00:00')", [],
    ).unwrap();
    let batch_id: i64 = db.conn.query_row("SELECT id FROM batches", [], |r| r.get(0)).unwrap();

    let result_id = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2, 3]).unwrap();
    let got = db.workflow_results().get_by_batch(batch_id).unwrap().expect("结果集");
    assert_eq!(got.id, result_id);

    let chapters = db.workflow_results().list_chapters(result_id).unwrap();
    assert_eq!(chapters.len(), 3);
    assert!(chapters.iter().all(|c| c.content.is_none()));

    // 写内容并验证
    let c2 = chapters.iter().find(|c| c.chapter_id == 2).unwrap();
    db.workflow_results().write_content(c2.id, "hello").unwrap();
    let updated = db.workflow_results().list_chapters(result_id).unwrap();
    let c2_again = updated.iter().find(|c| c.chapter_id == 2).unwrap();
    assert_eq!(c2_again.content.as_deref(), Some("hello"));
}

#[test]
fn write_content_unknown_id_is_a_noop() {
    let db = Db::open_in_memory().unwrap();
    db.workflow_results().write_content(9999, "x").unwrap(); // 不 panic
}
