use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewUpload};

fn make_upload(db: &Db) -> i64 {
    db.uploads()
        .insert(&NewUpload {
            sha256: "h".into(),
            filename: "x.txt".into(),
            byte_size: 0,
            file_path: "/tmp/x.txt".into(),
            original_text: "hello".into(),
            word_count: 0,
        })
        .unwrap()
}

fn make_data_asset(db: &Db, upload_id: i64) -> i64 {
    db.data_assets()
        .insert(&NewDataAsset { upload_id, title: "DA".into() })
        .unwrap()
}

#[test]
fn insert_persists_byte_range() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db);
    let da_id = make_data_asset(&db, upload_id);
    db.chapters()
        .insert(&nsc_core::models::NewChapter {
            data_asset_id: da_id,
            idx: 1,
            title: "Ch 1".into(),
            byte_start: 100,
            byte_end: 250,
            word_count: 0,
        })
        .unwrap();

    let segs = db.chapters().list_segments_by_data_asset(da_id).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].byte_start, Some(100));
    assert_eq!(segs[0].byte_end, Some(250));
    assert_eq!(segs[0].title, "Ch 1");
}

#[test]
fn insert_batch_persists_byte_ranges() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db);
    let da_id = make_data_asset(&db, upload_id);
    db.chapters()
        .insert_batch(&[
            nsc_core::models::NewChapter {
                data_asset_id: da_id,
                idx: 1,
                title: "A".into(),
                byte_start: 0,
                byte_end: 50,
                word_count: 0,
            },
            nsc_core::models::NewChapter {
                data_asset_id: da_id,
                idx: 2,
                title: "B".into(),
                byte_start: 50,
                byte_end: 120,
                word_count: 0,
            },
        ])
        .unwrap();

    let segs = db.chapters().list_segments_by_data_asset(da_id).unwrap();
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].byte_start, Some(0));
    assert_eq!(segs[0].byte_end, Some(50));
    assert_eq!(segs[1].byte_start, Some(50));
    assert_eq!(segs[1].byte_end, Some(120));
}

#[test]
fn replace_all_updates_byte_ranges() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db);
    let da_id = make_data_asset(&db, upload_id);
    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[nsc_core::models::NewChapter {
                data_asset_id: da_id,
                idx: 1,
                title: "old".into(),
                byte_start: 0,
                byte_end: 10,
                word_count: 0,
            }],
        )
        .unwrap();

    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[nsc_core::models::NewChapter {
                data_asset_id: da_id,
                idx: 1,
                title: "new".into(),
                byte_start: 200,
                byte_end: 400,
                word_count: 0,
            }],
        )
        .unwrap();

    let segs = db.chapters().list_segments_by_data_asset(da_id).unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].title, "new");
    assert_eq!(segs[0].byte_start, Some(200));
    assert_eq!(segs[0].byte_end, Some(400));
}

#[test]
fn list_segments_orders_by_idx_and_returns_all() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db);
    let da_id = make_data_asset(&db, upload_id);
    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[
                nsc_core::models::NewChapter {
                    data_asset_id: da_id,
                    idx: 3,
                    title: "third".into(),
                    byte_start: 200,
                    byte_end: 300,
                    word_count: 0,
                },
                nsc_core::models::NewChapter {
                    data_asset_id: da_id,
                    idx: 1,
                    title: "first".into(),
                    byte_start: 0,
                    byte_end: 100,
                    word_count: 0,
                },
                nsc_core::models::NewChapter {
                    data_asset_id: da_id,
                    idx: 2,
                    title: "second".into(),
                    byte_start: 100,
                    byte_end: 200,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    let segs = db.chapters().list_segments_by_data_asset(da_id).unwrap();
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].title, "first");
    assert_eq!(segs[1].title, "second");
    assert_eq!(segs[2].title, "third");
    assert_eq!(segs[0].byte_start, Some(0));
    assert_eq!(segs[2].byte_end, Some(300));
}