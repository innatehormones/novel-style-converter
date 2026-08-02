pub const SCHEMAS: &[(&str, &str)] = &[
    ("v1", include_str!("../../../../migrations/0001_init.sql")),
    ("v2", include_str!("../../../../migrations/0002_split_uploads.sql")),
    ("v3", include_str!("../../../../migrations/0003_chapter_byte_ranges.sql")),
    ("v4", include_str!("../../../../migrations/0004_data_assets.sql")),
    ("v5", include_str!("../../../../migrations/0005_chapters_data_asset_fk.sql")),
    ("v6", include_str!("../../../../migrations/0006_transformation_novels_data_asset_fk.sql")),
    ("v7", include_str!("../../../../migrations/0007_uploads_word_count.sql")),
];