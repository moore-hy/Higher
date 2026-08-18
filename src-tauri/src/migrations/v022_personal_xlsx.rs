use rusqlite::Connection;

/// DEV-0059.1 §9：Personal Source 支持 XLSX。
///
/// SQLite 无法直接修改 CHECK 约束，因此重建 personalization_sources 表
/// （file_type 增加 'xlsx'）；保留全部历史数据与索引。
/// 迁移期间 FK 由迁移框架统一 OFF/ON；personalization_source_chunks /
/// personalization_profile_sources 的 FK 引用的是表名（不变），RENAME 后自动恢复。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE personalization_sources_v022 (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            file_name           TEXT NOT NULL,
            file_type           TEXT NOT NULL CHECK (file_type IN ('txt','md','docx','pdf','xlsx')),
            relative_path       TEXT NOT NULL,
            sha256              TEXT NOT NULL,
            extracted_text_path TEXT NOT NULL DEFAULT '',
            status              TEXT NOT NULL DEFAULT 'extracted'
                                  CHECK (status IN ('imported','extracted','failed')),
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT INTO personalization_sources_v022
            (id, profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status, created_at, updated_at)
            SELECT id, profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status, created_at, updated_at
            FROM personalization_sources;
        DROP TABLE personalization_sources;
        ALTER TABLE personalization_sources_v022 RENAME TO personalization_sources;
        CREATE INDEX IF NOT EXISTS idx_psrc_profile ON personalization_sources(profile_id);
        "#,
    )
}
