use rusqlite::Connection;

/// V21 · Personal Planning Truth（DEV-0059）
///
/// 一次性收口的唯一主迁移：
/// 1. trusted_study_sessions 派生 VIEW（§6.1 可信统计统一；不是第二事实源）
/// 2. ai_runs + workflow_type/workflow_state/workflow_json（§6.8 Planner 显式状态机）
/// 3. personalization_profiles 重建为真正的 version rows（§8）
///    - UNIQUE(profile_id, version)；status draft/confirmed/superseded
///    - 每 profile 最多 1 confirmed / 1 draft（partial unique index）
///    - legacy confirmed/draft → v1（不凭空创建 confirmed）
/// 4. personalization_profile_sources 版本-来源快照关系（§8）
/// 5. goal_targets 通用目标核心（§11）+ 考研 postgraduate partial unique（reach/safety ≤1）
/// 6. planning_sources / planning_source_chunks（§13）
/// 7. planning_blueprints / planning_phases / planning_milestones（§14-16）
/// 8. planning_reviews（§17）
/// 9. evaluations → Evidence V1（§20）：session_id 可空 + source_kind/source_ref/trust_state
/// 10. tasks + origin/planning_blueprint_id/planning_phase_id/projection_key/user_modified_at（§21）
///
/// 永久规则：v001-v020 不改；保留所有历史数据；迁移幂等（版本系统语义）；
/// 真实 DB 迁移由 Human Runtime 验证。
pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        -- ============ 1) Trusted Session VIEW（§6.1；derived，非第二事实源） ============
        DROP VIEW IF EXISTS trusted_study_sessions;
        CREATE VIEW trusted_study_sessions AS
            SELECT * FROM study_sessions
            WHERE duration_review_state != 'needs_review';

        -- ============ 2) ai_runs workflow 列（§6.8） ============
        ALTER TABLE ai_runs ADD COLUMN workflow_type TEXT;
        ALTER TABLE ai_runs ADD COLUMN workflow_state TEXT;
        ALTER TABLE ai_runs ADD COLUMN workflow_json TEXT;
        CREATE INDEX IF NOT EXISTS idx_airun_workflow
            ON ai_runs(profile_id, conversation_id, workflow_type, id DESC);

        -- ============ 3) personalization_profiles → version rows（§8） ============
        CREATE TABLE personalization_profiles_v021 (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id          INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            version             INTEGER NOT NULL DEFAULT 1,
            md_content          TEXT NOT NULL DEFAULT '',
            structured_json     TEXT,
            status              TEXT NOT NULL DEFAULT 'draft'
                                    CHECK (status IN ('draft','confirmed','superseded')),
            based_on_version_id INTEGER,
            created_at          TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
            confirmed_at        TEXT,
            UNIQUE (profile_id, version)
        );
        -- legacy 迁移：confirmed→confirmed v1；仅 draft→draft v1；不得凭空创建 confirmed
        INSERT INTO personalization_profiles_v021
            (profile_id, version, md_content, structured_json, status, created_at, updated_at, confirmed_at)
        SELECT profile_id, version, md_content, structured_json,
               CASE WHEN status='confirmed' THEN 'confirmed' ELSE 'draft' END,
               COALESCE(last_compiled_at, datetime('now')),
               COALESCE(last_updated_at, datetime('now')),
               CASE WHEN status='confirmed' THEN COALESCE(last_updated_at, datetime('now')) END
        FROM personalization_profiles;
        DROP TABLE personalization_profiles;
        ALTER TABLE personalization_profiles_v021 RENAME TO personalization_profiles;
        -- 每 profile 最多 1 confirmed / 1 draft
        CREATE UNIQUE INDEX IF NOT EXISTS uq_pprof_one_confirmed
            ON personalization_profiles(profile_id) WHERE status='confirmed';
        CREATE UNIQUE INDEX IF NOT EXISTS uq_pprof_one_draft
            ON personalization_profiles(profile_id) WHERE status='draft';
        CREATE INDEX IF NOT EXISTS idx_pprof_profile ON personalization_profiles(profile_id, version DESC);

        -- ============ 4) PersonalProfile 版本-来源快照（§8） ============
        CREATE TABLE IF NOT EXISTS personalization_profile_sources (
            id                 INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_version_id INTEGER NOT NULL REFERENCES personalization_profiles(id) ON DELETE CASCADE,
            source_id          INTEGER NOT NULL REFERENCES personalization_sources(id) ON DELETE CASCADE,
            UNIQUE (profile_version_id, source_id)
        );
        CREATE INDEX IF NOT EXISTS idx_ppsrc_version ON personalization_profile_sources(profile_version_id);

        -- ============ 5) GoalTarget 通用核心（§11） ============
        CREATE TABLE IF NOT EXISTS goal_targets (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            scenario_type    TEXT NOT NULL DEFAULT 'generic',
            role             TEXT NOT NULL DEFAULT 'primary',
            title            TEXT NOT NULL,
            target_date      TEXT,
            data_json        TEXT NOT NULL DEFAULT '{}',
            provenance_json  TEXT NOT NULL DEFAULT '{}',
            status           TEXT NOT NULL DEFAULT 'candidate'
                                CHECK (status IN ('candidate','draft','active','historical','dismissed')),
            version          INTEGER NOT NULL DEFAULT 1,
            supersedes_id    INTEGER,
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
            activated_at     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_gt_profile ON goal_targets(profile_id, status, id DESC);
        -- 考研语义：postgraduate + active + reach / safety 每档案各 ≤1（其他 scenario 不受限）
        CREATE UNIQUE INDEX IF NOT EXISTS uq_gt_postgrad_reach ON goal_targets(profile_id)
            WHERE scenario_type='postgraduate' AND status='active' AND role='reach';
        CREATE UNIQUE INDEX IF NOT EXISTS uq_gt_postgrad_safety ON goal_targets(profile_id)
            WHERE scenario_type='postgraduate' AND status='active' AND role='safety';

        -- ============ 6) Planning Source（§13） ============
        CREATE TABLE IF NOT EXISTS planning_sources (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id     INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            source_kind    TEXT NOT NULL DEFAULT 'user_file'
                              CHECK (source_kind IN ('user_file','higher_ai','external_ai','manual','export_reimport')),
            original_name  TEXT NOT NULL,
            file_type      TEXT NOT NULL DEFAULT 'txt',
            original_path  TEXT NOT NULL DEFAULT '',
            sha256         TEXT NOT NULL DEFAULT '',
            status         TEXT NOT NULL DEFAULT 'imported'
                              CHECK (status IN ('imported','ready','failed','archived')),
            metadata_json  TEXT NOT NULL DEFAULT '{}',
            created_at     TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_psrc_profile ON planning_sources(profile_id);
        CREATE TABLE IF NOT EXISTS planning_source_chunks (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id   INTEGER NOT NULL REFERENCES planning_sources(id) ON DELETE CASCADE,
            profile_id  INTEGER NOT NULL,
            chunk_index INTEGER NOT NULL,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_psrcc_src ON planning_source_chunks(source_id, chunk_index);

        -- ============ 7) PlanningBlueprint / Phase / Milestone（§14-16） ============
        CREATE TABLE IF NOT EXISTS planning_blueprints (
            id                   INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id           INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            scenario_type        TEXT NOT NULL DEFAULT 'generic',
            version              INTEGER NOT NULL DEFAULT 1,
            status               TEXT NOT NULL DEFAULT 'draft'
                                    CHECK (status IN ('draft','active','superseded','rejected')),
            title                TEXT NOT NULL DEFAULT '',
            content_md           TEXT NOT NULL DEFAULT '',
            structured_json      TEXT,
            source_snapshot_json TEXT NOT NULL DEFAULT '{}',
            provenance_json      TEXT NOT NULL DEFAULT '{}',
            review_enabled       INTEGER NOT NULL DEFAULT 1,
            review_interval_days INTEGER NOT NULL DEFAULT 14 CHECK (review_interval_days >= 1),
            last_review_at       TEXT,
            next_review_at       TEXT,
            supersedes_id        INTEGER,
            created_at           TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at           TEXT NOT NULL DEFAULT (datetime('now')),
            activated_at         TEXT,
            UNIQUE (profile_id, version)
        );
        CREATE INDEX IF NOT EXISTS idx_pbp_profile ON planning_blueprints(profile_id, status, id DESC);
        -- 每 profile 最多一个 active Blueprint
        CREATE UNIQUE INDEX IF NOT EXISTS uq_pbp_one_active ON planning_blueprints(profile_id) WHERE status='active';

        CREATE TABLE IF NOT EXISTS planning_phases (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            blueprint_id  INTEGER NOT NULL REFERENCES planning_blueprints(id) ON DELETE CASCADE,
            phase_key     TEXT NOT NULL DEFAULT '',
            title         TEXT NOT NULL,
            start_date    TEXT,
            end_date      TEXT,
            objective_md  TEXT NOT NULL DEFAULT '',
            sort_order    INTEGER NOT NULL DEFAULT 0,
            status        TEXT NOT NULL DEFAULT 'planned'
                            CHECK (status IN ('planned','active','completed','cancelled')),
            data_json     TEXT NOT NULL DEFAULT '{}'
        );
        CREATE INDEX IF NOT EXISTS idx_pph_blueprint ON planning_phases(blueprint_id, sort_order);

        CREATE TABLE IF NOT EXISTS planning_milestones (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            blueprint_id     INTEGER NOT NULL REFERENCES planning_blueprints(id) ON DELETE CASCADE,
            phase_id         INTEGER REFERENCES planning_phases(id) ON DELETE SET NULL,
            milestone_key    TEXT NOT NULL DEFAULT '',
            title            TEXT NOT NULL,
            start_date       TEXT,
            end_date         TEXT,
            date_precision   TEXT NOT NULL DEFAULT 'unknown'
                                CHECK (date_precision IN ('day','range','month','unknown')),
            date_status      TEXT NOT NULL DEFAULT 'estimated'
                                CHECK (date_status IN ('estimated','official','user_confirmed','outdated','needs_review')),
            status           TEXT NOT NULL DEFAULT 'planned'
                                CHECK (status IN ('planned','completed','missed','cancelled')),
            provenance_json  TEXT NOT NULL DEFAULT '{}',
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_pms_blueprint ON planning_milestones(blueprint_id, status);

        -- ============ 8) Planning Review（§17） ============
        CREATE TABLE IF NOT EXISTS planning_reviews (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id            INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            blueprint_id          INTEGER REFERENCES planning_blueprints(id) ON DELETE SET NULL,
            period_start          TEXT NOT NULL,
            period_end            TEXT NOT NULL,
            trigger_type          TEXT NOT NULL DEFAULT 'scheduled'
                                    CHECK (trigger_type IN ('scheduled','milestone','manual','reality_change','anomaly')),
            status                TEXT NOT NULL DEFAULT 'due'
                                    CHECK (status IN ('due','running','waiting_approval','completed','skipped','failed')),
            evidence_snapshot_json TEXT NOT NULL DEFAULT '{}',
            assessment_md         TEXT NOT NULL DEFAULT '',
            recommendation_json   TEXT NOT NULL DEFAULT '{}',
            risk_state            TEXT NOT NULL DEFAULT 'unknown'
                                    CHECK (risk_state IN ('unknown','normal','attention','off_reach','near_safety','below_safety')),
            change_set_id         INTEGER,
            user_decision         TEXT NOT NULL DEFAULT '',
            resulting_blueprint_id INTEGER,
            created_at            TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at            TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at          TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_prev_profile ON planning_reviews(profile_id, status, id DESC);

        -- ============ 9) Evaluation → Evidence V1（§20） ============
        ALTER TABLE evaluations ADD COLUMN session_id INTEGER REFERENCES study_sessions(id) ON DELETE SET NULL;
        ALTER TABLE evaluations ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'user'
            CHECK (source_kind IN ('user','ai','import'));
        ALTER TABLE evaluations ADD COLUMN source_ref TEXT NOT NULL DEFAULT '';
        ALTER TABLE evaluations ADD COLUMN trust_state TEXT NOT NULL DEFAULT 'trusted'
            CHECK (trust_state IN ('trusted','needs_review'));
        CREATE INDEX IF NOT EXISTS idx_evals_session ON evaluations(session_id);

        -- ============ 10) Task Blueprint Ownership（§21） ============
        ALTER TABLE tasks ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'
            CHECK (origin IN ('manual','recurring','blueprint'));
        ALTER TABLE tasks ADD COLUMN planning_blueprint_id INTEGER
            REFERENCES planning_blueprints(id) ON DELETE SET NULL;
        ALTER TABLE tasks ADD COLUMN planning_phase_id INTEGER
            REFERENCES planning_phases(id) ON DELETE SET NULL;
        ALTER TABLE tasks ADD COLUMN projection_key TEXT NOT NULL DEFAULT '';
        ALTER TABLE tasks ADD COLUMN user_modified_at TEXT;
        CREATE INDEX IF NOT EXISTS idx_tasks_origin ON tasks(profile_id, origin, planned_date);
        -- §22.2 幂等：同 blueprint 同 projection item 不重复生成
        CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_projection
            ON tasks(planning_blueprint_id, projection_key) WHERE projection_key != '';
        "#
    )
}
