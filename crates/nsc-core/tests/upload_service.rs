use nsc_core::db::Db;
use nsc_core::upload::{upload_file, MAX_UPLOAD_BYTES};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn write_source(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(bytes).unwrap();
    p
}

fn sha_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

#[test]
fn upload_writes_file_and_db_row() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let bytes = b"chapter one content";
    let src = write_source(src_dir.path(), "a.txt", bytes);

    let u = upload_file(&db, &src, "a.txt", dest_dir.path()).unwrap();

    assert_eq!(u.sha256, sha_of(bytes));
    assert_eq!(u.filename, "a.txt");
    assert_eq!(u.byte_size, bytes.len() as i64);
    let stored = dest_dir.path().join(format!("{}.txt", u.sha256));
    assert_eq!(std::fs::read(&stored).unwrap(), bytes);
    let round = db.uploads().get(u.id).unwrap().unwrap();
    assert_eq!(round.original_text, "chapter one content");
}

#[test]
fn upload_dedupes_when_sha_already_present() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let bytes = b"same content";
    let src = write_source(src_dir.path(), "first.txt", bytes);

    let first = upload_file(&db, &src, "first.txt", dest_dir.path()).unwrap();

    let src2 = write_source(src_dir.path(), "second.txt", bytes);
    let second = upload_file(&db, &src2, "second.txt", dest_dir.path()).unwrap();

    assert_eq!(first.id, second.id, "dedup must reuse row id");
    assert_eq!(second.filename, "first.txt", "dedup returns original row's filename");
    let count = std::fs::read_dir(dest_dir.path()).unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn upload_rolls_back_file_when_db_insert_fails() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let bytes = b"will rollback";
    let src = write_source(src_dir.path(), "x.txt", bytes);

    db.execute_batch("DROP TABLE uploads").unwrap();

    let err = upload_file(&db, &src, "x.txt", dest_dir.path()).unwrap_err();
    assert!(format!("{err}").contains("database error"));

    let count = std::fs::read_dir(dest_dir.path()).unwrap().count();
    assert_eq!(count, 0, "orphan file should be removed on insert failure");
}

#[test]
fn upload_rejects_empty_filename() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let src = write_source(src_dir.path(), "a.txt", b"x");

    let err = upload_file(&db, &src, "   ", dest_dir.path()).unwrap_err();
    assert!(format!("{err}").contains("validation"));
}

#[test]
fn upload_rejects_missing_source() {
    let db = Db::open_in_memory().unwrap();
    let dest_dir = tempdir().unwrap();
    let bogus = dest_dir.path().join("nope.txt");

    let err = upload_file(&db, &bogus, "x.txt", dest_dir.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("io error") || msg.contains("not found") || msg.contains("系统找不到"),
        "unexpected error: {msg}");
}

#[test]
fn upload_rejects_directory_as_source() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let src = src_dir.path().join("a_subdir");
    std::fs::create_dir(&src).unwrap();

    let err = upload_file(&db, &src, "a.txt", dest_dir.path()).unwrap_err();
    assert!(format!("{err}").contains("validation"));
}

#[test]
fn upload_rejects_empty_file() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let src = write_source(src_dir.path(), "empty.txt", b"");

    let err = upload_file(&db, &src, "empty.txt", dest_dir.path()).unwrap_err();
    assert!(format!("{err}").contains("validation"));
}

#[test]
fn upload_rejects_oversized_file_without_reading() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let src = src_dir.path().join("big.txt");
    let f = std::fs::File::create(&src).unwrap();
    f.set_len(MAX_UPLOAD_BYTES + 1).unwrap();
    drop(f);

    let err = upload_file(&db, &src, "big.txt", dest_dir.path()).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("过大") || msg.to_lowercase().contains("too large"),
        "unexpected error: {msg}");
}

#[test]
fn upload_decodes_gbk_source() {
    let db = Db::open_in_memory().unwrap();
    let src_dir = tempdir().unwrap();
    let dest_dir = tempdir().unwrap();
    let gbk: Vec<u8> = vec![0xC4, 0xE3, 0xBA, 0xC3, 0x2C, 0xCA, 0xC0, 0xBD, 0xE7, 0x21];
    let src = write_source(src_dir.path(), "gbk.txt", &gbk);

    let u = upload_file(&db, &src, "gbk.txt", dest_dir.path()).unwrap();
    assert_eq!(u.original_text, "你好,世界!");
}