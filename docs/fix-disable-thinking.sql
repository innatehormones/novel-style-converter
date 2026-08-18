-- 修 model_configs 缺 disable_thinking 列 + 补 schema_versions 记录
-- 应用必须先关掉
ALTER TABLE model_configs ADD COLUMN disable_thinking INTEGER NOT NULL DEFAULT 0;
INSERT OR IGNORE INTO schema_versions (version, applied_at) VALUES ('0025_model_max_context', '2026-08-17T13:30:00Z');
INSERT OR IGNORE INTO schema_versions (version, applied_at) VALUES ('0026_model_disable_thinking', '2026-08-17T13:30:00Z');
