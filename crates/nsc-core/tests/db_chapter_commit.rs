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

fn make_data_asset(db: &Db, upload_id: i64) -> i64 {
    db.data_assets()
        .insert(&NewDataAsset { upload_id, title: "DA".into() })
        .unwrap()
}

#[test]
fn commit_replaces_existing_chapters() {
    // 旧"delete_all + insert_batch + renumber"三步路径已被
    // replace_all_for_data_asset 合并为单事务。
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "commit-replaces");
    let da_id = make_data_asset(&db, upload_id);

    // 初始两章(idx=1,2)。
    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[
                NewChapter {
                    data_asset_id: da_id,
                    idx: 1,
                    title: "旧1".into(),
                    byte_start: 0,
                    byte_end: 2,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da_id,
                    idx: 2,
                    title: "旧2".into(),
                    byte_start: 2,
                    byte_end: 4,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    // 用同 idx 再写一遍,模拟 commit 行为(整批替换 + renumber)。
    let n = db
        .chapters()
        .replace_all_for_data_asset(
            da_id,
            &[
                NewChapter {
                    data_asset_id: da_id,
                    idx: 1,
                    title: "新A".into(),
                    byte_start: 0,
                    byte_end: 1,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da_id,
                    idx: 2,
                    title: "新B".into(),
                    byte_start: 1,
                    byte_end: 2,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da_id,
                    idx: 3,
                    title: "新C".into(),
                    byte_start: 2,
                    byte_end: 3,
                    word_count: 0,
                },
            ],
        )
        .unwrap();
    assert_eq!(n, 3);

    let listed = db.chapters().list_by_data_asset(da_id).unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].title, "新A");
    assert_eq!(listed[0].idx, 1);
    assert_eq!(listed[1].idx, 2);
    assert_eq!(listed[2].idx, 3);
}

#[test]
fn count_chapters_via_list_by_data_asset() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "count");
    let da_id = make_data_asset(&db, upload_id);

    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[
                NewChapter {
                    data_asset_id: da_id,
                    idx: 1,
                    title: "1".into(),
                    byte_start: 0,
                    byte_end: 1,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da_id,
                    idx: 2,
                    title: "2".into(),
                    byte_start: 1,
                    byte_end: 2,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    let count: i64 = db.chapters().list_by_data_asset(da_id).unwrap().len() as i64;
    assert_eq!(count, 2);
}

#[test]
fn empty_upload_has_zero_count() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "empty");
    let da_id = make_data_asset(&db, upload_id);
    let count: i64 = db.chapters().list_by_data_asset(da_id).unwrap().len() as i64;
    assert_eq!(count, 0);
}

#[test]
fn replace_all_replaces_and_renumbers_atomically() {
    let db = Db::open_in_memory().unwrap();
    let upload_id = make_upload(&db, "atomic");
    let da_id = make_data_asset(&db, upload_id);

    // 旧章节:idx 1,2(renumber 已落地)
    db.chapters()
        .replace_all_for_data_asset(
            da_id,
            &[
                NewChapter {
                    data_asset_id: da_id,
                    idx: 1,
                    title: "旧1".into(),
                    byte_start: 0,
                    byte_end: 4,
                    word_count: 0,
                },
                NewChapter {
                    data_asset_id: da_id,
                    idx: 2,
                    title: "旧2".into(),
                    byte_start: 4,
                    byte_end: 8,
                    word_count: 0,
                },
            ],
        )
        .unwrap();

    let replacement = vec![
        NewChapter {
            data_asset_id: da_id,
            idx: 1,
            title: "新A".into(),
            byte_start: 0,
            byte_end: 1,
            word_count: 0,
        },
        NewChapter {
            data_asset_id: da_id,
            idx: 2,
            title: "新B".into(),
            byte_start: 1,
            byte_end: 2,
            word_count: 0,
        },
        NewChapter {
            data_asset_id: da_id,
            idx: 3,
            title: "新C".into(),
            byte_start: 2,
            byte_end: 3,
            word_count: 0,
        },
    ];
    let n = db
        .chapters()
        .replace_all_for_data_asset(da_id, &replacement)
        .unwrap();
    assert_eq!(n, 3);

    let listed = db.chapters().list_by_data_asset(da_id).unwrap();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[0].title, "新A");
    assert_eq!(listed[0].idx, 1);
    assert_eq!(listed[1].idx, 2);
    assert_eq!(listed[2].idx, 3);
    // 旧章节确实没了
    assert!(!listed.iter().any(|c| c.title == "旧1" || c.title == "旧2"));
}