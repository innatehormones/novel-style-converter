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

/// 与首个用例同构的最小 fixture,多插一章(id=4)供幂等并集用例使用。
/// 返回 batch_id。
fn seed_fixture(db: &Db) -> i64 {
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
         VALUES (1, 1, 'c1', 0, 10, 10), (1, 2, 'c2', 10, 20, 10), \
                (1, 3, 'c3', 20, 30, 10), (1, 4, 'c4', 30, 40, 10)",
        [],
    ).unwrap();
    db.conn.execute(
        "INSERT INTO batches (transformation_novel_id, label, on_failure_policy, status, created_at) \
         VALUES (1, NULL, 'pause_and_review', 'running', '2026-08-04T00:00:00+00:00')", [],
    ).unwrap();
    db.conn.query_row("SELECT id FROM batches", [], |r| r.get(0)).unwrap()
}

#[test]
fn create_for_batch_with_slots_is_idempotent_on_repeat_call() {
    let db = Db::open_in_memory().unwrap();
    let batch_id = seed_fixture(&db);

    let result_id_a = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2, 3]).unwrap();
    let result_id_b = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2, 3]).unwrap();

    // 同一 batch 重复调用命中 UNIQUE(batch_id),拿到同一行而不是新建。
    assert_eq!(result_id_a, result_id_b);
    assert_eq!(db.workflow_results().list_chapters(result_id_b).unwrap().len(), 3);
}

#[test]
fn create_for_batch_with_slots_adds_only_new_chapters_on_overlap() {
    let db = Db::open_in_memory().unwrap();
    let batch_id = seed_fixture(&db);

    let result_id_a = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2, 3]).unwrap();
    // 重叠但不同的章节列表:已存在的 (2,3) 被 IGNORE,只新增 4 → 并集 4 槽。
    let result_id_b = db.workflow_results().create_for_batch_with_slots(batch_id, &[2, 3, 4]).unwrap();

    assert_eq!(result_id_a, result_id_b);
    let chapters = db.workflow_results().list_chapters(result_id_b).unwrap();
    assert_eq!(chapters.len(), 4);
    assert_eq!(
        chapters.iter().map(|c| c.chapter_id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
    );
}

#[test]
fn write_content_by_chapter_writes_matching_slot() {
    let db = Db::open_in_memory().unwrap();
    let batch_id = seed_fixture(&db);
    let result_id = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2]).unwrap();

    db.workflow_results().write_content_by_chapter(batch_id, 2, "by-chapter").unwrap();

    let chapters = db.workflow_results().list_chapters(result_id).unwrap();
    let c2 = chapters.iter().find(|c| c.chapter_id == 2).unwrap();
    assert_eq!(c2.content.as_deref(), Some("by-chapter"));
    // 只写命中的槽,其它槽保持空。
    let c1 = chapters.iter().find(|c| c.chapter_id == 1).unwrap();
    assert!(c1.content.is_none());
}

#[test]
fn write_content_by_chapter_unknown_chapter_or_batch_is_a_noop() {
    let db = Db::open_in_memory().unwrap();
    let batch_id = seed_fixture(&db);
    let result_id = db.workflow_results().create_for_batch_with_slots(batch_id, &[1, 2]).unwrap();

    // 结果集存在但没有该章节槽 → 静默 noop。
    db.workflow_results().write_content_by_chapter(batch_id, 9999, "missing").unwrap();
    // 结果集不存在 → 子查询为空,静默 noop。
    db.workflow_results().write_content_by_chapter(99999, 2, "no-result").unwrap();

    let chapters = db.workflow_results().list_chapters(result_id).unwrap();
    assert_eq!(chapters.len(), 2);
    assert!(chapters.iter().all(|c| c.content.is_none()));
}
