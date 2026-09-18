//! GROUNDED LEARNING BRIDGE V1 · W3 —— Grounded Training Material Snapshot（§8）。
//!
//! 训练历史必须可复现：一次训练块所用的「真实学习材料快照」落库在
//! `training_block_runs.material_snapshot_json`（v043 新增，TEXT NULL）。
//! 用户事后重新导入 PDF 不得悄悄改变已创建训练块的语义。
//!
//! 这是**内容**，不是学习真相：本模块的任何读写都不产生 `LearningMoment` /
//! `Evidence` / `MemoryReview` / FSRS 推进（§8.3 证据边界）。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 快照里对一份真实材料的具体落点。
///
/// 只存**指针**（source / revision / section / chunk 的既有行 id），
/// **不存整篇文档** —— 快照保持有界，复用既有的文档结构化产物（§8.2）。
///
/// `section_id` 是 `Option`：`document_chunks.section_id` 在 v042 里可为 NULL，
/// 而「没有章节」与「章节 id = 0」是两件事（项目纪律：`None` 与 `0` 严格区分）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GroundedMaterialRef {
    pub source_id: i64,
    pub revision_id: i64,
    pub section_id: Option<i64>,
    pub chunk_id: i64,
}

/// 快照状态：材料可用 / 不可用（不可用时不带内容，只带原因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MaterialStatus {
    Ready,
    Unavailable,
}

/// 快照内容的生成来源类别 —— 决定它有多「权威」（§8.3：LLM 内容不是证据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedBy {
    Deterministic,
    AiNonAuthoritative,
    None,
}

/// 一次训练块真正用到的接地材料快照（§8.2 锁定形状）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GroundedTrainingMaterial {
    pub version: u32,
    pub status: MaterialStatus,
    pub protocol_id: String,
    pub prompt_text: Option<String>,
    pub cue_text: Option<String>,
    pub source_excerpt: Option<String>,
    pub reference_text: Option<String>,
    pub worked_steps: Vec<String>,
    pub hidden_step_index: Option<usize>,
    pub practice_prompt: Option<String>,
    pub transfer_prompt: Option<String>,
    pub generated_by: GeneratedBy,
    pub provenance: Vec<GroundedMaterialRef>,
    pub unavailable_reason: Option<String>,
}

/// 把快照写入某个训练块的 `material_snapshot_json`。
///
/// 边界纪律（§8 / §8.3）：
/// - **profile 隔离**：`block_run` 必须属于 `profile_id`，否则拒绝（provenance 不会跨档案泄漏）。
/// - **不可变**：已经写过快照（列非 NULL）的块拒绝再次写入（GB-MAT-05）。
/// - **不产生任何学习证据**：本函数只触碰 `training_block_runs` 的 JSON 列，
///   不触碰 `learning_moments` / `evidence` / `memory_reviews`，不推进 FSRS。
pub fn save_material_snapshot(
    conn: &Connection,
    profile_id: i64,
    block_run_id: i64,
    material: &GroundedTrainingMaterial,
) -> Result<(), String> {
    // 1) profile 隔离：确认块属于该 profile。
    let owner: Option<i64> = conn
        .query_row(
            "SELECT profile_id FROM training_block_runs WHERE id = ?1",
            rusqlite::params![block_run_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("load block run failed: {e}"))?;
    match owner {
        Some(owner) if owner == profile_id => {}
        Some(_) => return Err("block run does not belong to profile".to_string()),
        None => return Err("block run not found".to_string()),
    }

    // 2) 不可变：已存在快照则拒绝覆盖。
    let existing: Option<String> = conn
        .query_row(
            "SELECT material_snapshot_json FROM training_block_runs WHERE id = ?1",
            rusqlite::params![block_run_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("read snapshot failed: {e}"))?;
    if existing.is_some() {
        return Err("material snapshot already exists and is immutable".to_string());
    }

    // 3) 序列化并写入（仅碰 JSON 列，不触碰学习真相）。
    let json = serde_json::to_string(material).map_err(|e| format!("serialize snapshot: {e}"))?;
    conn.execute(
        "UPDATE training_block_runs \
         SET material_snapshot_json = ?1, updated_at = datetime('now') \
         WHERE id = ?2",
        rusqlite::params![json, block_run_id],
    )
    .map_err(|e| format!("write snapshot failed: {e}"))?;
    Ok(())
}

/// 读回某个训练块的快照。
///
/// **profile 隔离**：只有块属于 `profile_id` 时才返回快照；块不存在或属于别的
/// profile，一律返回 `None`（provenance 不会跨档案泄漏，GB-MAT-03）。
pub fn load_material_snapshot(
    conn: &Connection,
    profile_id: i64,
    block_run_id: i64,
) -> Result<Option<GroundedTrainingMaterial>, String> {
    let row: Option<(i64, Option<String>)> = conn
        .query_row(
            "SELECT profile_id, material_snapshot_json FROM training_block_runs WHERE id = ?1",
            rusqlite::params![block_run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| format!("read block run failed: {e}"))?;

    match row {
        Some((owner, json)) if owner == profile_id => match json {
            Some(json) => {
                let mat: GroundedTrainingMaterial = serde_json::from_str(&json)
                    .map_err(|e| format!("deserialize snapshot: {e}"))?;
                Ok(Some(mat))
            }
            None => Ok(None),
        },
        // 块不存在，或不属于该 profile —— 一律不返回快照。
        _ => Ok(None),
    }
}
