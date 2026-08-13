# Upload Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the frontend byte-passing bottleneck and make the upload write atomic by moving file IO to the backend, then adopting write-then-DB-insert-with-rollback ordering.

**Architecture:** Frontend uses Tauri 2 dialog plugin (`@tauri-apps/plugin-dialog`) to obtain the user-selected path; backend reads the file, decodes, hashes, dedups, then writes to disk + DB with best-effort atomic rollback. New `nsc_core::upload` module owns the business logic and is unit-testable without a Tauri runtime.

**Tech Stack:** Tauri 2 + `@tauri-apps/plugin-dialog` 2.x + `tauri-plugin-dialog` 2.x + rusqlite + `tempfile` (dev-dep) + `sha2 = "0.10"`.

---

## File Structure

New files:
- `crates/nsc-core/src/upload.rs` — pure upload service
- `crates/nsc-core/tests/upload_service.rs` — service unit tests

Modified files:
- `crates/nsc-core/src/lib.rs` — expose `pub mod upload`
- `crates/nsc-core/src/db/pool.rs` — add tiny `Db::execute_batch` test/admin helper
- `crates/nsc-core/Cargo.toml` — add `sha2 = "0.10"`
- `src-tauri/Cargo.toml` — add `tauri-plugin-dialog = "2"`; remove `sha2` (now in nsc-core)
- `src-tauri/src/lib.rs` — register dialog plugin
- `src-tauri/src/commands/uploads.rs` — refactor `upload_file` to delegate to service; new payload `{ file_path, filename }`; remove `sha256_hex`
- `src-tauri/capabilities/default.json` — add `dialog:default` permission
- `package.json` — add `@tauri-apps/plugin-dialog`
- `src/components/UploadDialog.vue` — use `open()` from dialog plugin; drop `FileReader`
- `src/ipc/commands.ts` — change `uploadFile` payload shape
- `src/stores/library.ts` — change `upload(input)` to `{ filePath, filename }`
- `src/views/Library.vue` — change `onUpload` to accept new shape
- `src/__tests__/commands.spec.ts` — update `uploadFile` shape test
- `src/__tests__/library.spec.ts` — update `upload` payload assertion
- `README.md` — document new flow + size limit

Decomposition principle: the service is the single source of truth for upload behavior; the Tauri command is a thin adapter; the frontend is just a path-picker + IPC caller. Each layer is testable independently.

---

## Task 1: Add `sha2` to nsc-core + tiny `Db::execute_batch` test helper

**Files:**
- Modify: `crates/nsc-core/Cargo.toml:1-30`
- Modify: `crates/nsc-core/src/db/pool.rs:14-66`

- [ ] **Step 1: Add `sha2` dependency to nsc-core**

In `crates/nsc-core/Cargo.toml`, in `[dependencies]`, add the line below after `chrono`:

```toml
sha2 = "0.10"
```

Verify the `[dependencies]` block now contains `sha2 = "0.10"`.

- [ ] **Step 2: Add `Db::execute_batch` helper for tests**

In `crates/nsc-core/src/db/pool.rs`, add the following method inside `impl Db`:

```rust
    /// Test/admin helper: execute a raw SQL batch. Production code paths
    /// must use the typed repos. Marked `#[doc(hidden)]` to keep it out
    /// of rendered docs but allow internal test access.
    #[doc(hidden)]
    pub fn execute_batch(&self, sql: &str) -> rusqlite::Result<()> {
        self.conn.execute_batch(sql)
    }
```

Place it immediately after `pub fn applied_schema_versions(...)` (after the closing brace of that method, before the `#[cfg(test)] mod tests` block).

- [ ] **Step 3: Run existing tests to confirm no regression**

Run:

```bash
cargo test -p nsc-core
```

Expected: all existing tests pass. `Db::execute_batch` is additive — nothing existing should change.

- [ ] **Step 4: Commit**

```bash
git add crates/nsc-core/Cargo.toml crates/nsc-core/src/db/pool.rs
git commit -m "feat(nsc-core): add sha2 dep and Db::execute_batch test helper"
```

---

## Task 2: Add `upload` service module (TDD: red → green)

**Files:**
- Create: `crates/nsc-core/src/upload.rs`
- Modify: `crates/nsc-core/src/lib.rs:1-10`

- [ ] **Step 1: Write the failing tests**

Create `crates/nsc-core/src/upload.rs` (empty for now — just compile placeholder so test file compiles):

```rust
// placeholder; replaced in Step 3
```

Create `crates/nsc-core/tests/upload_service.rs` with the tests below:

```rust
use nsc_core::db::Db;
use nsc_core::upload::{upload_file, MAX_UPLOAD_BYTES};
use std::io::Write;
use std::path::Path;
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
```

- [ ] **Step 2: Run tests to verify they fail (red)**

Run:

```bash
cargo test -p nsc-core --test upload_service
```

Expected: FAIL with `error[E0433]: failed to resolve: could not find 'upload' in 'nsc_core'` (because the module doesn't exist yet).

- [ ] **Step 3: Implement the `upload` service**

Replace the `crates/nsc-core/src/upload.rs` placeholder with:

```rust
//! Upload service: validate, decode, hash, dedup, write, insert with rollback.
//!
//! The Tauri command is a thin adapter; this module owns the business logic
//! so it can be unit-tested without a Tauri runtime.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::encoding::{decode_to_utf8, DecodedText};
use crate::error::{Error, Result};
use crate::models::{NewUpload, Upload};

/// Hard cap on a single uploaded file's size (bytes).
/// 256 MiB is generous for a novel txt while still bounding memory + IO.
pub const MAX_UPLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// Register a new upload from a user-chosen file path.
///
/// Atomicity model: file write succeeds, then DB insert is attempted; on
/// insert failure the file is removed so we never leave an orphan on disk.
/// On dedup hit, no new file is written.
///
/// * `source`  — user-chosen path to a regular file (validated)
/// * `filename` — display name; trimmed, must be non-empty
/// * `dest_dir` — directory to write `<sha>.txt` into (created if missing)
pub fn upload_file(
    db: &Db,
    source: &Path,
    filename: &str,
    dest_dir: &Path,
) -> Result<Upload> {
    let filename = filename.trim();
    if filename.is_empty() {
        return Err(Error::Validation("文件名不能为空".into()));
    }

    let meta = std::fs::metadata(source)?;
    if !meta.is_file() {
        return Err(Error::Validation(format!(
            "{} 不是文件",
            source.display()
        )));
    }
    let size = meta.len();
    if size == 0 {
        return Err(Error::Validation("文件为空".into()));
    }
    if size > MAX_UPLOAD_BYTES {
        return Err(Error::Validation(format!(
            "文件过大: {size} bytes (上限 {MAX_UPLOAD_BYTES} bytes)"
        )));
    }

    let bytes = std::fs::read(source)?;
    let DecodedText { text, .. } = decode_to_utf8(&bytes)
        .map_err(|e| Error::Validation(format!("解码失败: {e}")))?;

    let sha = sha256_hex(&bytes);

    if let Some(existing_id) = db.uploads().find_by_sha256(&sha)? {
        return db
            .uploads()
            .get(existing_id)?
            .ok_or_else(|| Error::Other("upload row missing".into()));
    }

    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(format!("{sha}.txt"));

    std::fs::write(&dest, &bytes)?;

    let insert = db.uploads().insert(&NewUpload {
        sha256: sha,
        filename: filename.to_string(),
        byte_size: size as i64,
        file_path: dest.to_string_lossy().to_string(),
        original_text: text,
    });

    let id = match insert {
        Ok(id) => id,
        Err(e) => {
            let _ = std::fs::remove_file(&dest);
            return Err(e);
        }
    };

    db.uploads()
        .get(id)?
        .ok_or_else(|| Error::Other("upload row missing".into()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    out.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_empty_is_known_value() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
```

In `crates/nsc-core/src/lib.rs`, add `pub mod upload;` to the existing module list (after `pub mod transformer;`, before `pub mod prompts;`):

```rust
pub mod error;
pub mod models;
pub mod db;
pub mod ai;
pub mod splitter;
pub mod encoding;
pub mod text;
pub mod transformer;
pub mod upload;
pub mod prompts;
pub mod cleaner;
```

- [ ] **Step 4: Run tests to verify they pass (green)**

Run:

```bash
cargo test -p nsc-core --test upload_service
```

Expected: all 9 tests pass. Plus the in-module `sha256_of_empty_is_known_value` test passes.

- [ ] **Step 5: Run full nsc-core test suite**

Run:

```bash
cargo test -p nsc-core
```

Expected: all tests pass (existing + new).

- [ ] **Step 6: Commit**

```bash
git add crates/nsc-core/src/upload.rs crates/nsc-core/src/lib.rs crates/nsc-core/tests/upload_service.rs
git commit -m "feat(nsc-core): add upload service with atomic rollback"
```

---

## Task 3: Refactor Tauri `upload_file` command to use service

**Files:**
- Modify: `src-tauri/src/commands/uploads.rs:1-90`
- Modify: `src-tauri/Cargo.toml:1-25`

- [ ] **Step 1: Remove `sha2` from src-tauri Cargo.toml**

In `src-tauri/Cargo.toml`, delete the line `sha2 = "0.10"` from `[dependencies]`.

- [ ] **Step 2: Replace `upload_file` command body to delegate to service**

Replace `src-tauri/src/commands/uploads.rs` with the content below (entire file). The new shape:

```rust
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::State;

use nsc_core::db::Db;
use nsc_core::encoding::read_text_file;
use nsc_core::models::Upload;
use nsc_core::upload;

/// Upload listing IPC DTO. Only carries upload self-fields; data_asset
/// related info belongs to DataAssetSummary (Task 7).
#[derive(Debug, Serialize)]
pub struct UploadSummary {
    pub id: i64,
    pub sha256: String,
    pub filename: String,
    pub byte_size: i64,
    pub uploaded_at: String,
    pub file_path: String,
}

#[derive(Debug, Deserialize)]
pub struct UploadFilePayload {
    pub file_path: String,
    pub filename: String,
}

fn uploads_dir() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("novel-style-converter").join("uploads")
}

fn to_summary(u: &Upload) -> UploadSummary {
    UploadSummary {
        id: u.id,
        sha256: u.sha256.clone(),
        filename: u.filename.clone(),
        byte_size: u.byte_size,
        uploaded_at: u.uploaded_at.to_rfc3339(),
        file_path: u.file_path.clone(),
    }
}

/// Read `upload.original_text`; DB field empty (legacy uploads) falls
/// back to reading the raw file from `file_path` and re-decoding.
pub fn read_upload_original_text(u: &Upload) -> Result<String, String> {
    if !u.original_text.is_empty() {
        return Ok(u.original_text.clone());
    }
    read_text_file(Path::new(&u.file_path)).map(|d| d.text)
}

#[tauri::command]
pub fn list_uploads(db: State<'_, Arc<Mutex<Db>>>) -> Result<Vec<UploadSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let ups = db.uploads().list().map_err(|e| e.to_string())?;
    Ok(ups.iter().map(to_summary).collect())
}

/// Register a new upload from a user-chosen file path. Delegates to
/// `nsc_core::upload::upload_file` for all business logic (decode, hash,
/// dedup, atomic write, DB insert with rollback).
#[tauri::command]
pub fn upload_file(
    db: State<'_, Arc<Mutex<Db>>>,
    payload: UploadFilePayload,
) -> Result<UploadSummary, String> {
    let dir = uploads_dir();
    let source = PathBuf::from(&payload.file_path);
    let db_guard = db.lock().map_err(|e| e.to_string())?;
    let u = upload::upload_file(&db_guard, &source, &payload.filename, &dir)
        .map_err(|e| e.to_string())?;
    Ok(to_summary(&u))
}

#[tauri::command]
pub fn delete_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    if let Some(da) = db.data_assets().find_by_upload(id).map_err(|e| e.to_string())? {
        if db.data_assets().is_locked(da.id).map_err(|e| e.to_string())? {
            return Err("upload 对应的 data_asset 已锁定,无法删除".into());
        }
    }
    let _ = std::fs::remove_file(&u.file_path);
    db.uploads().delete(id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_upload(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<UploadSummary, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let u = db.uploads().get(id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("upload {id} 不存在"))?;
    Ok(to_summary(&u))
}

#[tauri::command]
pub fn get_upload_text(db: State<'_, Arc<Mutex<Db>>>, id: i64) -> Result<Response, String> {
    let text = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let u = db.uploads().get(id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("upload {id} 不存在"))?;
        if !u.original_text.is_empty() {
            u.original_text.clone()
        } else {
            read_text_file(Path::new(&u.file_path))?.text
        }
    };
    Ok(Response::new(text.into_bytes()))
}

#[tauri::command]
pub fn update_upload_text(
    db: State<'_, Arc<Mutex<Db>>>,
    id: i64,
    text: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    if db.uploads().get(id).map_err(|e| e.to_string())?.is_none() {
        return Err(format!("upload {id} 不存在"));
    }
    if db.data_assets().find_by_upload(id).map_err(|e| e.to_string())?.is_some() {
        return Err("该 upload 已有 data_asset,无法修改原文。请先在数据资产页删除后再修改。".into());
    }
    db.uploads().set_original_text(id, &text).map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Verify it compiles**

Run:

```bash
cargo build -p nsc-desktop
```

Expected: `Finished` with no errors. The `tauri::ipc::Response` import is still needed for `get_upload_text`.

- [ ] **Step 4: Run src-tauri-adjacent tests**

Run:

```bash
cargo test -p nsc-core
```

Expected: still passes (the service is unchanged from Task 2's perspective).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/commands/uploads.rs
git commit -m "refactor(tauri): delegate upload_file to nsc_core::upload service"
```

---

## Task 4: Add Tauri dialog plugin (Rust + npm + capability)

**Files:**
- Modify: `src-tauri/Cargo.toml:1-25`
- Modify: `src-tauri/src/lib.rs:1-60`
- Modify: `src-tauri/capabilities/default.json`
- Modify: `package.json:1-30`

- [ ] **Step 1: Add `tauri-plugin-dialog` Rust dep**

In `src-tauri/Cargo.toml`, in `[dependencies]`, add:

```toml
tauri-plugin-dialog = "2"
```

- [ ] **Step 2: Register the plugin in `lib.rs`**

In `src-tauri/src/lib.rs`, change the `tauri::Builder::default()` chain to also register the dialog plugin. Insert `.plugin(tauri_plugin_dialog::init())` between `tauri::Builder::default()` and `.manage(db)`:

```rust
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .manage(queue)
        .setup(|_app| Ok(()))
        .invoke_handler(tauri::generate_handler![
```

- [ ] **Step 3: Add `dialog:default` permission to capability file**

Read `src-tauri/capabilities/default.json` first to confirm its current shape. Append `"dialog:default"` to the existing `permissions` array, preserving order. If the file currently looks like:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability",
  "windows": ["main"],
  "permissions": [
    "core:default"
  ]
}
```

then change to:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capability",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default"
  ]
}
```

(If additional permissions are already listed, slot `"dialog:default"` next to `"core:default"` for readability.)

- [ ] **Step 4: Add `@tauri-apps/plugin-dialog` to `package.json`**

In `package.json`, in `"dependencies"`, add (alphabetical ordering — after `@tauri-apps/api`):

```json
"@tauri-apps/plugin-dialog": "^2.0.0",
```

- [ ] **Step 5: Install the new frontend dep**

Run:

```bash
pnpm install
```

Expected: pnpm adds `@tauri-apps/plugin-dialog` to `node_modules` and updates `pnpm-lock.yaml`. No errors.

- [ ] **Step 6: Verify Rust build still works**

Run:

```bash
cargo build -p nsc-desktop
```

Expected: compiles, plugin registers without complaint.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/lib.rs src-tauri/capabilities/default.json package.json pnpm-lock.yaml
git commit -m "feat(tauri): add dialog plugin for upload path selection"
```

---

## Task 5: Refactor frontend `UploadDialog.vue` to use dialog plugin

**Files:**
- Modify: `src/components/UploadDialog.vue:1-95`

- [ ] **Step 1: Replace `UploadDialog.vue` template + script**

Replace the contents of `src/components/UploadDialog.vue` with the version below. Key changes:
- Drop `<input type="file">` and `FileReader`.
- Call `open()` from `@tauri-apps/plugin-dialog` to get a path.
- Emit `{ filePath, filename }` instead of `{ filename, bytes }`.

```vue
<template>
  <Dialog v-model:open="open" title="上传 .txt 文件" :width="480">
    <div class="row">
      <label>文本文件 *</label>
      <Button kind="primary" :disabled="picking" @click="onPick">
        {{ picking ? '选择中...' : (filePath ? '重新选择' : '选择文件') }}
      </Button>
    </div>
    <div v-if="fileInfo" class="file-info">
      {{ fileInfo.name }} · {{ fileInfo.path }}
    </div>
    <div v-if="error" class="error">{{ error }}</div>
    <template #footer>
      <Button :disabled="submitting" @click="open = false">取消</Button>
      <Button kind="primary" :loading="submitting" :disabled="!canSubmit || submitting" @click="onSubmit">上传</Button>
    </template>
  </Dialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import Dialog from './ui/Dialog.vue';
import Button from './ui/Button.vue';

const open = defineModel<boolean>('open', { required: true });
const emit = defineEmits<{ submit: [{ filePath: string; filename: string }] }>();

const filePath = ref('');
const filename = ref('');
const error = ref<string | null>(null);
const submitting = ref(false);
const picking = ref(false);

const fileInfo = computed(() =>
  filePath.value ? { name: filename.value, path: filePath.value } : null,
);

const canSubmit = computed(() => filePath.value !== '');

watch(open, (v) => {
  if (v) {
    filePath.value = '';
    filename.value = '';
    error.value = null;
    submitting.value = false;
    picking.value = false;
  }
});

async function onPick() {
  error.value = null;
  picking.value = true;
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Text', extensions: ['txt'] }],
    });
    if (typeof selected === 'string') {
      filePath.value = selected;
      const segs = selected.split(/[\\\/]/);
      filename.value = segs[segs.length - 1] || 'uploaded.txt';
    }
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    picking.value = false;
  }
}

function onSubmit() {
  if (!canSubmit.value) return;
  error.value = null;
  submitting.value = true;
  try {
    emit('submit', { filePath: filePath.value, filename: filename.value });
    open.value = false;
  } finally {
    submitting.value = false;
  }
}
</script>

<style scoped>
.row {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
  gap: 12px;
}
.row label {
  width: 90px;
  font-size: 14px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.file-info {
  font-size: 12px;
  color: var(--text-muted);
  margin-bottom: 12px;
  word-break: break-all;
}
.error {
  color: var(--danger);
  font-size: 12px;
  margin-bottom: 8px;
}
</style>
```

- [ ] **Step 2: Verify TypeScript types**

Run:

```bash
pnpm exec vue-tsc --noEmit
```

Expected: no type errors related to the new `open` import or emit signature.

- [ ] **Step 3: Commit**

```bash
git add src/components/UploadDialog.vue
git commit -m "refactor(upload): dialog plugin picks path, backend reads file"
```

---

## Task 6: Update IPC wrapper + types + store + view + tests

**Files:**
- Modify: `src/ipc/commands.ts:36-40`
- Modify: `src/stores/library.ts:24-30`
- Modify: `src/views/Library.vue` (the `onUpload` function)
- Modify: `src/__tests__/commands.spec.ts:99-110`
- Modify: `src/__tests__/library.spec.ts:54-75`

- [ ] **Step 1: Update IPC wrapper payload shape**

In `src/ipc/commands.ts`, replace the `uploadFile` function:

```ts
export function uploadFile(payload: { file_path: string; filename: string }): Promise<UploadSummary> {
  return invoke<UploadSummary>('upload_file', { payload });
}
```

(Field names are snake_case to match the inner DTO convention; outer arg name is `payload` per existing convention.)

- [ ] **Step 2: Update store `upload` signature**

In `src/stores/library.ts`, replace the `upload` function inside `defineStore('library', ...)`:

```ts
  async function upload(input: { file_path: string; filename: string }): Promise<UploadSummary> {
    uploading.value = true;
    try {
      const result = await ipcUploadFile(input);
      await load();
      return result;
    } finally {
      uploading.value = false;
    }
  }
```

- [ ] **Step 3: Update `Library.vue` `onUpload` to use new shape**

In `src/views/Library.vue`, the `onUpload` function is the consumer passed to `@submit`. Find it and replace:

```ts
// onUpload is the translation layer between Vue-camelCase dialog emit
// and snake-case IPC DTO. Dialog emits { filePath, filename }; we re-pack
// to { file_path, filename } before handing to the store / IPC.
async function onUpload(input: { filePath: string; filename: string }) {
  try {
    await store.upload({ file_path: input.filePath, filename: input.filename });
  } catch (e: unknown) {
    alert(e instanceof Error ? e.message : String(e));
  }
}
```

(No other change to the template — `@submit="onUpload"` stays.)

- [ ] **Step 4: Update `commands.spec.ts` test**

In `src/__tests__/commands.spec.ts`, find the `it('uploadFile sends snake_case payload', ...)` test inside the `Upload IPC wrappers` describe block. Replace it with:

```ts
  it('uploadFile sends snake_case payload with file_path', async () => {
    const sample: UploadSummary = {
      id: 1, sha256: 'x', filename: 'A.txt', byte_size: 3, uploaded_at: '2026-07-26T00:00:00Z', file_path: '/x',
    };
    vi.mocked(invoke).mockResolvedValueOnce(sample);
    const r = await uploadFile({ file_path: 'C:/tmp/A.txt', filename: 'A.txt' });
    expect(r).toEqual(sample);
    expect(invoke).toHaveBeenCalledWith('upload_file', { payload: { file_path: 'C:/tmp/A.txt', filename: 'A.txt' } });
  });
```

- [ ] **Step 5: Update `library.spec.ts` tests**

In `src/__tests__/library.spec.ts`, find the `describe('library store', ...)` block. Make the following replacements:

For the test about `upload` happy path (around line 60), replace the call site:

```ts
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'upload_file') return Promise.resolve({ ...sampleUpload, id: 7 });
      if (cmd === 'list_uploads') return Promise.resolve([{ ...sampleUpload, id: 7 }]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected cmd: ${cmd}`));
    });
    const store = useLibraryStore();
    const result = await store.upload({ file_path: '/tmp/A.txt', filename: 'A.txt' });
    expect(invoke).toHaveBeenCalledWith('upload_file', { payload: { file_path: '/tmp/A.txt', filename: 'A.txt' } });
    expect(result.id).toBe(7);
    expect(store.uploads[0].id).toBe(7);
    expect(store.uploading).toBe(false);
```

For the test about `uploading=true` during pending, the first call uses `{ filename: 'big.txt', bytes: [9] }`. Replace with:

```ts
    const pending = store.upload({ file_path: '/tmp/big.txt', filename: 'big.txt' });
```

For the rejection assertion in the same test, replace `store.upload({ filename: 'broken.txt', bytes: [] })` with:

```ts
    await expect(
      store.upload({ file_path: '/tmp/broken.txt', filename: 'broken.txt' }),
    ).rejects.toThrow('boom');
```

- [ ] **Step 6: Run frontend tests**

Run:

```bash
pnpm test
```

Expected: all vitest specs pass.

- [ ] **Step 7: Run TypeScript check**

Run:

```bash
pnpm exec vue-tsc --noEmit
```

Expected: no type errors.

- [ ] **Step 8: Commit**

```bash
git add src/ipc/commands.ts src/stores/library.ts src/views/Library.vue src/__tests__/commands.spec.ts src/__tests__/library.spec.ts
git commit -m "refactor(upload): switch store/view/tests to file_path payload"
```

---

## Task 7: Run full test matrix + smoke

**Files:** none (verification only)

- [ ] **Step 1: Backend tests**

Run:

```bash
cargo test -p nsc-core
```

Expected: every existing test plus the 9 new `upload_service` tests pass.

- [ ] **Step 2: Frontend tests**

Run:

```bash
pnpm test
```

Expected: all vitest specs pass.

- [ ] **Step 3: TypeScript check**

Run:

```bash
pnpm exec vue-tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Vite build (frontend)**

Run:

```bash
pnpm build
```

Expected: `vue-tsc` passes + `vite build` produces `dist/`. No warnings about missing modules.

- [ ] **Step 5: Cargo workspace build**

Run:

```bash
cargo build --workspace
```

Expected: `nsc-core` and `nsc-desktop` both compile.

- [ ] **Step 6: Smoke script (4s GUI-independent)**

Run:

```bash
pwsh scripts/smoke.ps1
```

Expected: smoke exits 0 (it doesn't actually exercise upload, but confirms the app starts and IPC layer loads).

---

## Task 8: Update README

**Files:**
- Modify: `README.md` (the section that describes the upload flow + size limit if mentioned)

- [ ] **Step 1: Find and update the upload description**

Search `README.md` for the section covering upload. Likely section header mentions "小说导入 / 上传 .txt / sha256 去重".

If the README has a "功能概述" subsection listing upload behavior, replace any wording that mentions frontend FileReader or ArrayBuffer bytes with the new flow. Add a sentence:

```markdown
- 上传链路: 前端通过 `tauri-plugin-dialog` 选择文件路径, 后端 `nsc_core::upload` 自行读取、解码、SHA-256 去重并写入 `%APPDATA%/novel-style-converter/uploads/<sha>.txt`, DB 插入失败时回滚删除物理文件, 避免孤儿文件。单文件上限 256 MiB (`MAX_UPLOAD_BYTES`).
```

(Slot this into the most relevant spot in the existing feature list, preserving tone.)

- [ ] **Step 2: Re-verify no placeholders**

Run:

```bash
rg 'TBD|TODO|FIXME|XXX|implement later|fill in details' docs/superpowers/plans/2026-07-31-upload-refactor.md
```

Expected: no matches.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: describe new upload flow (backend reads, atomic write, 256MB cap)"
```

---

## Self-Review

**1. Spec coverage:**
- Frontend no longer reads bytes -> Task 5 (`UploadDialog.vue` uses dialog plugin; tasks 5/6 keep payload free of `bytes`)
- Backend reads file -> Task 2 service signature `(db, source, filename, dest_dir)` + Task 3 Tauri command delegates
- Atomic write -> Task 2 Step 3 implementation does `write -> insert -> remove on failure`
- Size limit -> `MAX_UPLOAD_BYTES` constant in Task 2 + enforced before read into memory
- SHA-256 dedup preserved -> service still calls `find_by_sha256`
- Encoding decode preserved -> service still calls `decode_to_utf8`
- Tests for new behavior -> 9 service tests in Task 2 + updated wrapper/store specs in Task 6
- README updated -> Task 8

**2. Placeholder scan:** No "TBD/TODO/implement later/fill in details" in plan body.

**3. Type consistency:**
- Service: `pub fn upload_file(db: &Db, source: &Path, filename: &str, dest_dir: &Path) -> Result<Upload>` - used identically in Task 2 implementation, Task 3 Tauri command, Task 2 tests.
- Tauri DTO: `UploadFilePayload { file_path: String, filename: String }` - matches TS payload `{ file_path: string; filename: string }` in Tasks 5/6.
- Frontend wrapper: `uploadFile({ file_path, filename })` - store `upload({ file_path, filename })` - view `onUpload({ file_path, filename })` - all consistent.
- TS test assertions use `{ file_path: '/tmp/A.txt', filename: 'A.txt' }` consistently across `commands.spec.ts` and `library.spec.ts`.
- `MAX_UPLOAD_BYTES` is `pub` in `nsc_core::upload`, accessed by the test as `nsc_core::upload::MAX_UPLOAD_BYTES`.

No mismatches found.
