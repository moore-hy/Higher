//! HIGHER COGNITIVE CORE V1.2 §12 / §13 — Memory 持久化层（**不引用 fsrs**）。
//!
//! 本文件只做 DB 读写与 profile 归属校验；**排程计算一律不在本层**
//! （那是 `engine.rs` 的职责）。所有查询都以 `profile_id` 为一等过滤条件，
//! 保证 ME-09（no cross-profile read）。

use rusqlite::{params, Connection};

use super::types::{
    normalize_utc, DueMemoryUnit, MemoryKind, MemoryUnit, NewMemoryUnit, ReviewRating,
    DEFAULT_DESIRED_RETENTION,
};

/// 校验某个被引用的父行属于指定 profile（与 `cognitive::learning_moment` 同纪律）。
fn assert_owned(
    conn: &Connection,
    table: &'static str,
    id: i64,
    profile_id: i64,
    label: &str,
) -> Result<(), String> {
    let sql = format!("SELECT profile_id FROM {table} WHERE id = ?1");
    let found: Option<Option<i64>> =
        match conn.query_row(&sql, params![id], |row| row.get::<_, Option<i64>>(0)) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(e.to_string()),
        };
    match found {
        None => Err(format!("{label}不存在（id={id}）")),
        Some(None) => Err(format!("{label}没有档案归属（id={id}），拒绝写入")),
        Some(Some(p)) if p != profile_id => Err(format!(
            "跨档案引用被拒绝：{label} id={id} 属于 profile {p}，不是 profile {profile_id}"
        )),
        Some(Some(_)) => Ok(()),
    }
}

/// 档案存在性校验。
pub fn assert_profile_exists(conn: &Connection, profile_id: i64) -> Result<(), String> {
    let found: Option<i64> = match conn.query_row(
        "SELECT id FROM study_profiles WHERE id = ?1",
        params![profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };
    match found {
        Some(_) => Ok(()),
        None => Err(format!("学习档案不存在（id={profile_id}）")),
    }
}

const UNIT_COLUMNS: &str = "id, profile_id, linked_learning_item_id, memory_key, memory_kind,
     stability, difficulty, retrievability, last_review_at, next_review_at,
     desired_retention, review_count, lapse_count, fsrs_state_json, created_at, updated_at";

fn row_to_unit(row: &rusqlite::Row<'_>) -> Result<MemoryUnit, String> {
    let kind_raw: String = row.get(4).map_err(|e| e.to_string())?;
    let memory_kind =
        MemoryKind::parse(&kind_raw).ok_or_else(|| format!("未知 memory_kind：{kind_raw}"))?;
    let state_raw: String = row.get(13).map_err(|e| e.to_string())?;
    let fsrs_state_json: serde_json::Value =
        serde_json::from_str(&state_raw).unwrap_or_else(|_| serde_json::json!({}));

    Ok(MemoryUnit {
        id: row.get(0).map_err(|e| e.to_string())?,
        profile_id: row.get(1).map_err(|e| e.to_string())?,
        linked_learning_item_id: row.get(2).map_err(|e| e.to_string())?,
        memory_key: row.get(3).map_err(|e| e.to_string())?,
        memory_kind,
        stability: row.get(5).map_err(|e| e.to_string())?,
        difficulty: row.get(6).map_err(|e| e.to_string())?,
        retrievability: row.get(7).map_err(|e| e.to_string())?,
        last_review_at: row.get(8).map_err(|e| e.to_string())?,
        next_review_at: row.get(9).map_err(|e| e.to_string())?,
        desired_retention: row.get(10).map_err(|e| e.to_string())?,
        review_count: row.get(11).map_err(|e| e.to_string())?,
        lapse_count: row.get(12).map_err(|e| e.to_string())?,
        fsrs_state_json,
        created_at: row.get(14).map_err(|e| e.to_string())?,
        updated_at: row.get(15).map_err(|e| e.to_string())?,
    })
}

fn row_to_unit_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryUnit> {
    row_to_unit(row).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })
}

/// 严格创建一条 MemoryUnit（重复 `(profile, item, memory_key)` → 报错，让唯一约束可见）。
pub fn insert_memory_unit(conn: &Connection, unit: &NewMemoryUnit) -> Result<MemoryUnit, String> {
    assert_profile_exists(conn, unit.profile_id)?;
    assert_owned(
        conn,
        "learning_items",
        unit.linked_learning_item_id,
        unit.profile_id,
        "学习项",
    )?;

    if unit.memory_key.trim().is_empty() {
        return Err("memory_key 不能为空".to_string());
    }

    conn.execute(
        "INSERT INTO memory_units
            (profile_id, linked_learning_item_id, memory_key, memory_kind, desired_retention)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            unit.profile_id,
            unit.linked_learning_item_id,
            unit.memory_key.trim(),
            unit.memory_kind.as_str(),
            unit.effective_desired_retention(),
        ],
    )
    .map_err(|e| {
        let s = e.to_string();
        if s.contains("UNIQUE") {
            format!(
                "同一档案 + 同一学习项 + 同一 memory_key 已存在（profile={}, item={}, key={}）",
                unit.profile_id, unit.linked_learning_item_id, unit.memory_key
            )
        } else {
            s
        }
    })?;

    get_memory_unit_scoped(conn, unit.profile_id, conn.last_insert_rowid())?
        .ok_or_else(|| "写入后读取失败".to_string())
}

/// 按 id 读取（**必须**同时带 profile，避免跨档案读取）。
pub fn get_memory_unit_scoped(
    conn: &Connection,
    profile_id: i64,
    id: i64,
) -> Result<Option<MemoryUnit>, String> {
    let sql = format!("SELECT {UNIT_COLUMNS} FROM memory_units WHERE id = ?1 AND profile_id = ?2");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(params![id, profile_id])
        .map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(row) => Ok(Some(row_to_unit(row)?)),
        None => Ok(None),
    }
}

/// 查找 `(profile, item, memory_key)` 是否已存在。
pub fn find_memory_unit(
    conn: &Connection,
    profile_id: i64,
    linked_learning_item_id: i64,
    memory_key: &str,
) -> Result<Option<MemoryUnit>, String> {
    let sql = format!(
        "SELECT {UNIT_COLUMNS} FROM memory_units
         WHERE profile_id = ?1 AND linked_learning_item_id = ?2 AND memory_key = ?3"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query(params![
            profile_id,
            linked_learning_item_id,
            memory_key.trim()
        ])
        .map_err(|e| e.to_string())?;
    match rows.next().map_err(|e| e.to_string())? {
        Some(row) => Ok(Some(row_to_unit(row)?)),
        None => Ok(None),
    }
}

/// 某学习项下的全部 MemoryUnit（稳定顺序：id ASC）。
pub fn list_memory_units_for_item(
    conn: &Connection,
    profile_id: i64,
    linked_learning_item_id: i64,
) -> Result<Vec<MemoryUnit>, String> {
    let sql = format!(
        "SELECT {UNIT_COLUMNS} FROM memory_units
         WHERE profile_id = ?1 AND linked_learning_item_id = ?2 ORDER BY id ASC"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![profile_id, linked_learning_item_id],
            row_to_unit_raw,
        )
        .map_err(|e| e.to_string())?;
    collect(rows)
}

/// 某档案的全部 MemoryUnit（id ASC；压力计算用）。
pub fn list_all_memory_units(
    conn: &Connection,
    profile_id: i64,
) -> Result<Vec<MemoryUnit>, String> {
    let sql =
        format!("SELECT {UNIT_COLUMNS} FROM memory_units WHERE profile_id = ?1 ORDER BY id ASC");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id], row_to_unit_raw)
        .map_err(|e| e.to_string())?;
    collect(rows)
}

/// 已到期队列（`next_review_at <= now`，按到期时间升序，再 id 升序）。
pub fn list_due_memory_units(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
    limit: i64,
) -> Result<Vec<DueMemoryUnit>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let now = normalize_utc(now_utc);
    let sql = format!(
        "SELECT {UNIT_COLUMNS},
                (SELECT name FROM learning_items li WHERE li.id = memory_units.linked_learning_item_id),
                CAST(julianday(?2) - julianday(memory_units.next_review_at) AS INTEGER)
         FROM memory_units
         WHERE profile_id = ?1
           AND next_review_at IS NOT NULL
           AND next_review_at <= ?2
         ORDER BY next_review_at ASC, id ASC
         LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, now, limit], |row| {
            let unit = row_to_unit_raw(row)?;
            let label: Option<String> = row.get(16)?;
            let overdue_days: Option<i64> = row.get(17)?;
            let status = unit_status_label(&unit);
            Ok(DueMemoryUnit {
                unit,
                learning_item_label: label,
                overdue_days: overdue_days.unwrap_or(0),
                status,
            })
        })
        .map_err(|e| e.to_string())?;
    collect(rows)
}

/// 尚未到期队列（`next_review_at > now`，按到期时间升序，再 id 升序）。
///
/// §25 Memory 页「下一次复习」区块使用。与 `list_due_memory_units` **同形**
/// （`DueMemoryUnit`），只是窗口相反：这里 `overdue_days` 为**负数**
/// （见该结构的文档：「负数 = 尚未到期」），UI 不得把它当作「已逾期」渲染。
///
/// `next_review_at IS NULL` 的 unit（从未复习、尚未进入排程）**不进入**本列表：
/// 它没有可展示的下一次复习时刻，凭空给它排一个日期就是编造（§36）。
pub fn list_upcoming_memory_units(
    conn: &Connection,
    profile_id: i64,
    now_utc: &str,
    limit: i64,
) -> Result<Vec<DueMemoryUnit>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let now = normalize_utc(now_utc);
    let sql = format!(
        "SELECT {UNIT_COLUMNS},
                (SELECT name FROM learning_items li WHERE li.id = memory_units.linked_learning_item_id),
                CAST(julianday(?2) - julianday(memory_units.next_review_at) AS INTEGER)
         FROM memory_units
         WHERE profile_id = ?1
           AND next_review_at IS NOT NULL
           AND next_review_at > ?2
         ORDER BY next_review_at ASC, id ASC
         LIMIT ?3"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, now, limit], |row| {
            let unit = row_to_unit_raw(row)?;
            let label: Option<String> = row.get(16)?;
            let overdue_days: Option<i64> = row.get(17)?;
            let status = unit_status_label(&unit);
            Ok(DueMemoryUnit {
                unit,
                learning_item_label: label,
                overdue_days: overdue_days.unwrap_or(0),
                status,
            })
        })
        .map_err(|e| e.to_string())?;
    collect(rows)
}

/// Memory 页行状态标签（§11 Stability 轴口径的展示投影：new / due / stable）。
pub fn unit_status_label(unit: &MemoryUnit) -> String {
    if !unit.has_completed_review() {
        return "new".to_string();
    }
    if unit.below_desired_retention() {
        return "due".to_string();
    }
    if unit.review_count >= 2 {
        return "stable".to_string();
    }
    // 已有一次以上复习但尚未达到 stable 的条件：仍然按「到期」语义呈现，
    // 绝不用一个伪造的中间百分比。
    "due".to_string()
}

/// 插入一条复习账本行。
#[allow(clippy::too_many_arguments)]
pub fn insert_memory_review(
    conn: &Connection,
    profile_id: i64,
    memory_unit_id: i64,
    learning_moment_id: Option<i64>,
    rating: ReviewRating,
    reviewed_at: &str,
    elapsed_days: i64,
    scheduled_days: i64,
    state_before_json: &serde_json::Value,
    state_after_json: &serde_json::Value,
) -> Result<i64, String> {
    conn.execute(
        "INSERT INTO memory_reviews
            (profile_id, memory_unit_id, learning_moment_id, rating, reviewed_at,
             elapsed_days, scheduled_days, state_before_json, state_after_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            profile_id,
            memory_unit_id,
            learning_moment_id,
            rating.as_str(),
            normalize_utc(reviewed_at),
            elapsed_days,
            scheduled_days,
            state_before_json.to_string(),
            state_after_json.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 更新 MemoryUnit 的缓存排程字段（**FSRS 结果缓存**，不是真相本身）。
#[allow(clippy::too_many_arguments)]
pub fn update_unit_scheduling(
    conn: &Connection,
    profile_id: i64,
    memory_unit_id: i64,
    stability: f64,
    difficulty: f64,
    retrievability: f64,
    last_review_at: &str,
    next_review_at: &str,
    fsrs_state_json: &serde_json::Value,
    increment_review: bool,
    increment_lapse: bool,
) -> Result<(), String> {
    let affected = conn
        .execute(
            "UPDATE memory_units
                SET stability = ?3,
                    difficulty = ?4,
                    retrievability = ?5,
                    last_review_at = ?6,
                    next_review_at = ?7,
                    fsrs_state_json = ?8,
                    review_count = review_count + ?9,
                    lapse_count = lapse_count + ?10,
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![
                memory_unit_id,
                profile_id,
                stability,
                difficulty,
                retrievability,
                normalize_utc(last_review_at),
                normalize_utc(next_review_at),
                fsrs_state_json.to_string(),
                if increment_review { 1 } else { 0 },
                if increment_lapse { 1 } else { 0 },
            ],
        )
        .map_err(|e| e.to_string())?;
    if affected != 1 {
        return Err(format!(
            "MemoryUnit 更新影响行数异常（{affected}）；profile={profile_id} unit={memory_unit_id}"
        ));
    }
    Ok(())
}

/// 读取某 MemoryUnit 的复习账本（新→旧）。
pub fn list_reviews_for_unit(
    conn: &Connection,
    profile_id: i64,
    memory_unit_id: i64,
    limit: i64,
) -> Result<Vec<super::types::MemoryReview>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let mut stmt = conn
        .prepare(
            "SELECT id, profile_id, memory_unit_id, learning_moment_id, rating, reviewed_at,
                    elapsed_days, scheduled_days, state_before_json, state_after_json, created_at
             FROM memory_reviews
             WHERE profile_id = ?1 AND memory_unit_id = ?2
             ORDER BY reviewed_at DESC, id DESC LIMIT ?3",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![profile_id, memory_unit_id, limit], |row| {
            let rating_raw: String = row.get(4)?;
            let before_raw: String = row.get(8)?;
            let after_raw: String = row.get(9)?;
            let rating = ReviewRating::parse(&rating_raw).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("未知 rating：{rating_raw}"),
                    )),
                )
            })?;
            Ok(super::types::MemoryReview {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                memory_unit_id: row.get(2)?,
                learning_moment_id: row.get(3)?,
                rating,
                reviewed_at: row.get(5)?,
                elapsed_days: row.get(6)?,
                scheduled_days: row.get(7)?,
                state_before_json: serde_json::from_str(&before_raw)
                    .unwrap_or_else(|_| serde_json::json!({})),
                state_after_json: serde_json::from_str(&after_raw)
                    .unwrap_or_else(|_| serde_json::json!({})),
                created_at: row.get(10)?,
            })
        })
        .map_err(|e| e.to_string())?;
    collect(rows)
}

/// 默认期望保留率（对外暴露，便于上层避免硬编码）。
pub fn default_desired_retention() -> f64 {
    DEFAULT_DESIRED_RETENTION
}

fn collect<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>, String>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}
