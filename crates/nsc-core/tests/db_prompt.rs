use nsc_core::db::Db;
use nsc_core::prompts;

#[test]
fn seed_inserts_only_when_empty() {
    let db = Db::open_in_memory().unwrap();

    db.seed_builtin_prompts().unwrap();
    let first = db.prompts().list().unwrap();
    assert_eq!(first.len(), prompts::builtin_prompts().len());
    assert!(first.iter().all(|p| p.is_builtin));

    db.seed_builtin_prompts().unwrap();
    let second = db.prompts().list().unwrap();
    assert_eq!(second.len(), first.len());
}
