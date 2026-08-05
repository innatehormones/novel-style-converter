use nsc_core::db::Db;
use nsc_core::models::{NewDataAsset, NewTransformationNovel, NewUpload};

#[test]
fn tn_references_data_asset_id() {
    let db = Db::open_in_memory().unwrap();
    let uid = db.uploads().insert(&NewUpload {
        sha256: "h".into(), filename: "n.txt".into(),
        byte_size: 10, file_path: "/p".into(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "n".into() }).unwrap();
    let tn_id = db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "compact".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    let got = db.transformation_novels().get(tn_id).unwrap().unwrap();
    assert_eq!(got.data_asset_id, da_id);
    // data_asset 不再有"业务锁"概念;TN 引用状态由 list_with_upload.tn_count 实时统计。
}

#[test]
fn list_by_data_asset() {
    let db = Db::open_in_memory().unwrap();
    let uid = db.uploads().insert(&NewUpload {
        sha256: "a".into(), filename: "n.txt".into(),
        byte_size: 1, file_path: "/p".into(),
        original_text: String::new(),
        word_count: 0,
    }).unwrap();
    let da_id = db.data_assets().insert(&NewDataAsset { upload_id: uid, title: "n".into() }).unwrap();
    db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "a".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    db.transformation_novels().insert(&NewTransformationNovel {
        data_asset_id: da_id, title: "b".into(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }).unwrap();
    let list = db.transformation_novels().list_by_data_asset(da_id).unwrap();
    assert_eq!(list.len(), 2);
}
