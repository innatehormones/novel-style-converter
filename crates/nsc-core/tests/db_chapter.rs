use nsc_core::db::Db;
use nsc_core::models::{NewChapter, NewDataAsset, NewUpload};

fn make_upload(db: &Db, name: &str) -> i64 {
    db.uploads()
        .insert(&NewUpload {
            sha256: format!("h-{name}"),
            filename: format!("{name}.txt"),
            byte_size: 0,
            file_path: format!("/tmp/{name}.txt"),
            original_text: format!("{name}-text"),
            word_count: 0,
        })
        .unwrap()
}

fn make_data_asset(db: &Db, upload_id: i64, title: &str) -> i64 {
    db.data_assets()
        .insert(&NewDataAsset { upload_id, title: title.to_string() })
        .unwrap()
}

#[test]
fn insert_batch_and_list_by_data_asset() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "batch-list");
    let data_asset_id = make_data_asset(&db, upload_id, "DA");

    let chapters: Vec<NewChapter> = (1..=3)
        .map(|i| NewChapter {
            data_asset_id,
            idx: i,
            title: format!("Ch {i}"),
            byte_start: (i as i64 - 1) * 30,
            byte_end: i as i64 * 30,
            word_count: 4,
        })
        .collect();
    db.chapters().insert_batch(&chapters).unwrap();

    let list = db.chapters().list_by_data_asset(data_asset_id).unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].idx, 1);
    assert!(list[0].word_count > 0);
}

#[test]
fn delete_upload_cascades_chapters() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "cascade");
    let data_asset_id = make_data_asset(&db, upload_id, "DA");

    db.chapters().insert(&NewChapter {
        data_asset_id,
        idx: 1,
        title: "Ch 1".into(),
        byte_start: 0,
        byte_end: 6,
        word_count: 2,
    }).unwrap();

    assert_eq!(db.chapters().list_by_data_asset(data_asset_id).unwrap().len(), 1);
    db.uploads().delete(upload_id).unwrap();
    assert!(db.chapters().list_by_data_asset(data_asset_id).unwrap().is_empty());
}

#[test]
fn renumber_no_gaps_after_delete() {
    // renumber 已被 replace_all_for_data_asset 取代:整批重插时 idx 自然拍成 1..N。
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "renum");
    let data_asset_id = make_data_asset(&db, upload_id, "DA");

    // 初始 3 章(模拟旧数据 idx=1,2,3)。
    db.chapters()
        .replace_all_for_data_asset(
            data_asset_id,
            &[
                NewChapter {
                    data_asset_id,
                    idx: 1,
                    title: "Ch 1".into(),
                    byte_start: 0,
                    byte_end: 10,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 2,
                    title: "Ch 2".into(),
                    byte_start: 10,
                    byte_end: 20,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 3,
                    title: "Ch 3".into(),
                    byte_start: 20,
                    byte_end: 30,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    // 删掉 idx=2 的那章(用 replace_all_for_data_asset 重插,跳过 idx=2 的内容)。
    db.chapters()
        .replace_all_for_data_asset(
            data_asset_id,
            &[
                NewChapter {
                    data_asset_id,
                    idx: 1,
                    title: "Ch 1".into(),
                    byte_start: 0,
                    byte_end: 10,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 2,
                    title: "Ch 3".into(),
                    byte_start: 10,
                    byte_end: 20,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    let list = db.chapters().list_by_data_asset(data_asset_id).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].idx, 1);
    assert_eq!(list[1].idx, 2);
}

#[test]
fn renumber_preserves_order_by_idx_then_id() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "renum-order");
    let data_asset_id = make_data_asset(&db, upload_id, "DA");

    // 用非连续 idx 插入,然后用 replace_all_for_data_asset 触发 renumber。
    let _ = db
        .chapters()
        .replace_all_for_data_asset(
            data_asset_id,
            &[
                NewChapter {
                    data_asset_id,
                    idx: 2,
                    title: "B".into(),
                    byte_start: 0,
                    byte_end: 5,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 5,
                    title: "E".into(),
                    byte_start: 5,
                    byte_end: 10,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 1,
                    title: "A".into(),
                    byte_start: 10,
                    byte_end: 15,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id,
                    idx: 7,
                    title: "G".into(),
                    byte_start: 15,
                    byte_end: 20,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    let list = db.chapters().list_by_data_asset(data_asset_id).unwrap();
    assert_eq!(
        list.iter().map(|c| c.idx).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        list.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
        vec!["A", "B", "E", "G"]
    );
}

#[test]
fn renumber_empty_upload_returns_zero() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "empty");
    let data_asset_id = make_data_asset(&db, upload_id, "DA");

    // 空批次的 replace_all_for_data_asset 返回插入数量 0。
    let n = db.chapters().replace_all_for_data_asset(data_asset_id, &[]).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn renumber_only_target_upload() {
    let db = Db::open_in_memory().unwrap();
    let u1 = make_upload(&db, "u1");
    let u2 = make_upload(&db, "u2");
    let da1 = make_data_asset(&db, u1, "DA1");
    let da2 = make_data_asset(&db, u2, "DA2");

    db.chapters()
        .replace_all_for_data_asset(
            da1,
            &[
                NewChapter {
                    data_asset_id: da1,
                    idx: 1,
                    title: "u1 ch1".into(),
                    byte_start: 0,
                    byte_end: 10,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da1,
                    idx: 2,
                    title: "u1 ch2".into(),
                    byte_start: 10,
                    byte_end: 20,
                    word_count: 0,
                },
            ],
        )
        .unwrap();
    db.chapters()
        .replace_all_for_data_asset(
            da2,
            &[
                NewChapter {
                    data_asset_id: da2,
                    idx: 1,
                    title: "u2 ch1".into(),
                    byte_start: 0,
                    byte_end: 10,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da2,
                    idx: 2,
                    title: "u2 ch2".into(),
                    byte_start: 10,
                    byte_end: 20,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    // 重建 da1(只留 1 章)→ renumber 仅作用于 da1。
    let n = db
        .chapters()
        .replace_all_for_data_asset(
            da1,
            &[NewChapter {
                data_asset_id: da1,
                idx: 1,
                title: "u1 ch1".into(),
                byte_start: 0,
                byte_end: 10,
                word_count: 0,
            }],
        )
        .unwrap();
    assert_eq!(n, 1);

    let u1_list = db.chapters().list_by_data_asset(da1).unwrap();
    let u2_list = db.chapters().list_by_data_asset(da2).unwrap();
    assert_eq!(
        u1_list.iter().map(|c| c.idx).collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        u2_list.iter().map(|c| c.idx).collect::<Vec<_>>(),
        vec![1, 2]
    );
}