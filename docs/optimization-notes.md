# novel-style-converter Business Description (post-refactor)

## Two independent data blocks

1. Upload: raw file with sha + filename + size + original_text (editable for cleaning).
2. DataAsset + Chapter: a parsed package. Chapters carry the actual text inline via chapters.body (no more byte-range slicing of the upload).

A single upload can produce many data assets, and the data assets survive the upload being deleted. The link data_assets.upload_id is informational only (no FK, no UNIQUE).

## Key data model

- uploads: sha256, filename, byte_size, file_path, original_text, word_count
- data_assets: upload_id (informational), title, parsed_at, source_filename
- chapters: data_asset_id, idx, title, body TEXT, word_count
- transformation_novels: data_asset_id (fan-out)
- batches / transformation_chapters / workflow_results: scheduler + result set

## Delete semantics

- Delete upload: preview_upload_deletion returns the list of derived data assets; the deletion is non-cascading. The UI shows the list and lets the user decide.
- Delete data_asset: cascades chapters + transformation_novels via FK.
- Delete transformation_novel: removes tn + its transformation_chapters only.
- Delete chapter: only allowed when no transformation references it.

## App start to first page

- Tauri starts JobQueue and BatchScheduler workers.
- Vue loads library view (default = Upload tab).
- Upload tab reads only uploads; "Upload .txt" calls upload_file.
- "Parse chapters" navigates to ParseView: splitter runs locally, edits (markers, suppressed, titleOverrides) live in the chapters store.
- "Save as data asset" calls commit_data_asset(title, [{title, content}]). Server writes each chapter row with body directly.
- DataAsset view shows chapters + their body, no byte-range slicing.

## Transform flow

- BatchScheduler.create_workflow(spec) -> same-batch serial dispatch.
- JobQueue reads chapter.body and pushes to AiProvider.
- prev_original / next_original are taken from chapters.body directly (no upload.original_text lookup).

## Why the change

- The old byte-range model conflated bytes with chars in CJK text and made upload deletion implicit-cascade chapters of unrelated work.
- The new model keeps each chapter self-contained and decouples uploads from data assets.

## Open improvements (not done)

- TransformationNovelDetail could move into a Pinia store to remove ad-hoc refs.
- Status changes could be event-driven instead of 1s polling.
- chapters store markers / suppressed / titleOverrides use string keys today; could switch to chapter_id once IDs are exposed end-to-end.

## Test status

cargo test -p nsc-core runs an ignored placeholder per file. Old tests referenced byte-coordinate assertions and now-deprecated API shapes. They are flagged for rewrite against migrations/0015_chapter_body.sql and the new repo methods.
