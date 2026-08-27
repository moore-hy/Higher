//! DEV-0075 §五.1 · profile.rs——PersonalProfile 门面（方案 B 映射复用）。
//!
//! 映射决策（DEV-0075_CONFLICT_REPORT §五）：
//! - `PersonalProfile`（任务书）→ 既有 `UserContext` 八节模型，持久化于
//!   `personalization_profiles.user_context_json`（v026 列）；
//! - 不新建 `personal_profiles` 表（与 v021 version rows 语义重复）。
//!
//! 职责（PI-001 / PI-005）：
//! - 读：confirmed 优先 → draft（复用 `load_user_context`）；
//! - 提案：对话内 AI 建档产出新 UserContext → **draft version 行**（不动
//!   confirmed 正式值——未确认不进 Profile）；
//! - 确认：用户确认后 `confirm`（v021 既有确认流：draft → confirmed）。
//!
//! 纪律（§原则3 / §十一）：AI 不得直接覆盖 confirmed 档案；一切更新走
//! draft 提案 + 用户确认两步。

use rusqlite::Connection;

use super::user_context::UserContext;

/// PI-001/PI-005：读取当前 PersonalProfile（映射 = load_user_context）。
pub fn load_profile(conn: &Connection, profile_id: i64) -> UserContext {
    super::load_user_context(conn, profile_id)
}

/// PI-005：对话建档提案——把 AI 产出的（合并后）UserContext 写为
/// **draft 行**的 user_context_json（不触碰 confirmed 正式值；
/// 读取端 confirmed 优先，故 confirmed 在时提案对 Decision 不可见）。
/// v021 部分唯一索引「每 profile 至多 1 draft」→ 已有 draft 即 UPDATE
///（提案始终是最新一版待确认）；返回 draft 行 id。
pub fn propose_profile_update(
    conn: &Connection,
    profile_id: i64,
    ctx: &UserContext,
) -> Result<i64, String> {
    let json = serde_json::to_string(ctx).map_err(|e| e.to_string())?;
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='draft'",
            rusqlite::params![profile_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        conn.execute(
            "UPDATE personalization_profiles SET user_context_json=?2, updated_at=datetime('now')
             WHERE id=?1",
            rusqlite::params![id, json],
        )
        .map_err(|e| format!("Profile 提案更新失败：{e}"))?;
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO personalization_profiles
            (profile_id, version, md_content, structured_json, status, user_context_json)
         VALUES (?1, COALESCE((SELECT MAX(version) FROM personalization_profiles
                               WHERE profile_id=?1), 0) + 1, '', NULL, 'draft', ?2)",
        rusqlite::params![profile_id, json],
    )
    .map_err(|e| format!("Profile 提案写库失败：{e}"))?;
    Ok(conn.last_insert_rowid())
}

/// PI-005：用户确认——最新 draft 升 confirmed（v021 既有确认流；
/// confirm 内部会把旧 confirmed 置 superseded）。
pub fn confirm_profile(conn: &Connection, profile_id: i64) -> Result<(), String> {
    crate::repository::personalization::PersonalizationRepository::new(conn)
        .confirm(profile_id)
}

/// 是否存在待确认的 Profile 提案（UI 可提示「AI 更新了你的档案，待确认」）。
pub fn has_pending_proposal(conn: &Connection, profile_id: i64) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM personalization_profiles
         WHERE profile_id=?1 AND status='draft' AND user_context_json IS NOT NULL)",
        rusqlite::params![profile_id],
        |r| r.get::<_, i64>(0),
    )
    .map(|v| v == 1)
    .unwrap_or(false)
}
