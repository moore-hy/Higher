//! Training Runtime 仓储与事务（REAL LEARNING ENGINE V1 · §11 / §13–§20）。
//!
//! # 事务边界是本模块的核心产出
//!
//! 三个写路径各自是一个**不可分割**的事实：
//!
//! ```text
//! create_training_run    §19  StudySession + TrainingRun + 全部块 + DIRECT 意图消费
//! record_interaction     §15  interaction + LearningMoment + memory_review + memory_unit
//! complete_training_run  §20  TrainingRun.completed + StudySession 终结
//! ```
//!
//! 绝不允许出现的中间态（§15 / §19）：
//!
//! ```text
//! 有 StudySession 但没有 TrainingRun
//! 有 TrainingRun 但没有块列表
//! 有 interaction 但没有 moment
//! 有 moment 但 FSRS 只更新了一半
//! ```
//!
//! # §15 为什么必须 BEGIN IMMEDIATE
//!
//! 交互管线是「先读（查幂等键）再写」。默认的 DEFERRED 事务会在**第一次写**时才
//! 升级为写锁，于是两个并发重试可能都读到「没有该键」，然后都尝试插入 ——
//! 唯一索引会挡住第二次，但错误会以约束冲突的形式出现，而不是返回既有结果。
//! IMMEDIATE 在事务开始时就取得写锁，让「查幂等键 → 写」成为真正的临界区。
//!
//! 事务通过 `execute_batch("BEGIN IMMEDIATE")` 显式开启，之后**复用同一个连接**
//! 完成全部读写，最后 `COMMIT` / `ROLLBACK`。这里刻意不构造第二个 `Connection`
//! 句柄：同一个 SQLite 连接上开一个事务，所有语句都必须走这同一个连接。
//!
//! # §21 AI 默认静默
//!
//! 本模块不引用任何 LLM / provider / agent 符号。AI 是否存在，对训练流程的
//! **可用性**没有任何影响：确定性路径（`VerificationMethod::Deterministic`）
//! 在没有本地模型、云端关闭时照常工作。

use rusqlite::{params, Connection};

use crate::cognitive::decision::DecisionMode;
use crate::cognitive::learning_moment::{
    record_learning_moment, LearningMomentType, MomentSourceType, NewLearningMoment,
};
use crate::cognitive::protocol::{find as find_protocol, CompletionRuleKind, ProtocolId};
use crate::cognitive::session_composer::TrainingSessionPlan;
use crate::memory::engine::record_review_from_moment_in_tx;
use crate::repository::active_learning_intent::clear_active_intent_in_tx;
use crate::repository::study_session::StudySessionRepository;

use super::completion::{evaluate_completion, BlockInteractionFact, CompletionFacts};
use super::grounded_material::{save_material_snapshot, GroundedTrainingMaterial};
use super::types::{
    derive_moment_type, is_recall_compatible, is_recall_moment, transition_block_status,
    transition_run_status, validate_block_invariant, BlockAdvanceIntent, BlockProgression,
    EffectSummary, InteractionResult, TrainingBlockRun, TrainingBlockStatus, TrainingError,
    TrainingErrorCode, TrainingInteraction, TrainingRun, TrainingRunStatus, VerificationMethod,
    FSRS_SKIP_BLOCK_IS_BREAK, FSRS_SKIP_EVIDENCE_TOO_LOW, FSRS_SKIP_NON_AUTHORITATIVE,
    FSRS_SKIP_NOT_RECALL_MOMENT, FSRS_SKIP_NO_MEMORY_UNIT, FSRS_SKIP_NO_MOMENT,
};

/// §18：训练派生的 moment 统一来源前缀。
pub const TRAINING_SOURCE_PREFIX: &str = "training_interaction:";

pub fn training_source_id(interaction_id: i64) -> String {
    format!("{TRAINING_SOURCE_PREFIX}{interaction_id}")
}

// ============================ 行映射 ============================

const RUN_COLUMNS: &str = "id, profile_id, study_session_id, learning_item_id, mode, status,
     current_block_ordinal, plan_snapshot_json, started_at, ended_at, created_at, updated_at";

fn conversion_error(idx: usize, what: &str, raw: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        idx,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("未知 {what}：{raw}"),
        )),
    )
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrainingRun> {
    let mode_raw: String = row.get(4)?;
    let status_raw: String = row.get(5)?;
    let mode = match mode_raw.as_str() {
        "direct" => DecisionMode::Direct,
        "copilot" => DecisionMode::Copilot,
        "autopilot" => DecisionMode::Autopilot,
        _ => return Err(conversion_error(4, "mode", &mode_raw)),
    };
    let status = TrainingRunStatus::parse(&status_raw)
        .ok_or_else(|| conversion_error(5, "status", &status_raw))?;
    Ok(TrainingRun {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        study_session_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        mode,
        status,
        current_block_ordinal: row.get(6)?,
        plan_snapshot_json: row.get(7)?,
        started_at: row.get(8)?,
        ended_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

const BLOCK_COLUMNS: &str = "id, profile_id, training_run_id, ordinal, protocol_id, is_break,
     goal, planned_minutes, memory_unit_id, status, started_at, ended_at, created_at, updated_at";

fn row_to_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrainingBlockRun> {
    let protocol_raw: Option<String> = row.get(4)?;
    let status_raw: String = row.get(9)?;
    let protocol_id = match protocol_raw.as_deref() {
        None => None,
        Some(raw) => {
            Some(ProtocolId::parse(raw).ok_or_else(|| conversion_error(4, "protocol_id", raw))?)
        }
    };
    let status = TrainingBlockStatus::parse(&status_raw)
        .ok_or_else(|| conversion_error(9, "block status", &status_raw))?;
    Ok(TrainingBlockRun {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        training_run_id: row.get(2)?,
        ordinal: row.get(3)?,
        protocol_id,
        is_break: row.get::<_, i64>(5)? != 0,
        goal: row.get(6)?,
        planned_minutes: row.get(7)?,
        memory_unit_id: row.get(8)?,
        status,
        started_at: row.get(10)?,
        ended_at: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const INTERACTION_COLUMNS: &str = "id, profile_id, training_run_id, block_run_id, client_action_id,
     interaction_type, prompt_text, user_response_text, hint_level, result, effect_summary_json,
     created_at";

fn row_to_interaction(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrainingInteraction> {
    let result_raw: Option<String> = row.get(9)?;
    let result = match result_raw.as_deref() {
        None => None,
        Some(raw) => {
            Some(InteractionResult::parse(raw).ok_or_else(|| conversion_error(9, "result", raw))?)
        }
    };
    Ok(TrainingInteraction {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        training_run_id: row.get(2)?,
        block_run_id: row.get(3)?,
        client_action_id: row.get(4)?,
        interaction_type: row.get(5)?,
        prompt_text: row.get(6)?,
        user_response_text: row.get(7)?,
        hint_level: row.get(8)?,
        result,
        effect_summary_json: row.get(10)?,
        created_at: row.get(11)?,
    })
}

// ============================ 前置校验 ============================

fn assert_profile_exists(conn: &Connection, profile_id: i64) -> Result<(), TrainingError> {
    let found: Option<i64> = match conn.query_row(
        "SELECT id FROM study_profiles WHERE id = ?1",
        params![profile_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TrainingError::db(e)),
    };
    match found {
        Some(_) => Ok(()),
        None => Err(TrainingError::new(
            TrainingErrorCode::ProfileNotFound,
            format!("学习档案不存在（id={profile_id}）"),
        )),
    }
}

fn assert_item_in_profile(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
) -> Result<(), TrainingError> {
    let owner: Option<i64> = match conn.query_row(
        "SELECT profile_id FROM learning_items WHERE id = ?1",
        params![learning_item_id],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TrainingError::db(e)),
    };
    match owner {
        Some(o) if o == profile_id => Ok(()),
        _ => Err(TrainingError::new(
            TrainingErrorCode::LearningItemNotInProfile,
            format!("学习项不存在或不属于该档案（item={learning_item_id}, profile={profile_id}）"),
        )),
    }
}

/// HOTFIX-01 FIX C 的**单一**判据：这个块现在是不是「当前活跃块」。
///
/// ```text
/// run.status               == Active
/// block.status             == Active
/// run.current_block_ordinal == block.ordinal
/// ```
///
/// 抽成一个函数而不是内联，是因为它有多个调用点（写入事实、审计测试），
/// 而它们必须共享**同一个**判据 —— 否则就会出现「后端允许写、前端却禁用」
/// 这类两个真相源的经典分裂。
///
/// 注意 `current_block_ordinal` 的比较：FIX E 之后，`current_block_ordinal`
/// 指向的是「下一个待开始的块」，而那个块是 **Pending**。所以「是当前块」
/// 与「是活跃块」在 FIX E 语义下**恰好**同时成立，缺一不可。
pub fn block_is_current_active(run: &TrainingRun, block: &TrainingBlockRun) -> bool {
    run.status == TrainingRunStatus::Active
        && block.status == TrainingBlockStatus::Active
        && run.current_block_ordinal == Some(block.ordinal)
}

// ============================ §11 回忆块 → MemoryUnit 绑定 ============================
/// §11 锁定的绑定规则。**绝不猜测**。
///
/// ```text
/// 1. 查该档案 + 该学习项的到期 MemoryUnit（next_review_at <= now）
/// 2. 排序 next_review_at ASC, id ASC
/// 3. 有一个或多个到期 → 绑定第一个
/// 4. 否则若该学习项**恰好只有一个** MemoryUnit → 绑定它
/// 5. 否则 → None
/// ```
///
/// 第 5 步是关键纪律：多个都未到期时，选哪一个都是**猜**。宁可不绑定 ——
/// 不绑定只意味着「这次不推进 FSRS」，属于 unknown/unbound，**不是失败**（§11 / §50）。
pub fn resolve_recall_memory_unit(
    conn: &Connection,
    profile_id: i64,
    learning_item_id: i64,
    now_utc: &str,
) -> Result<Option<i64>, TrainingError> {
    let now = crate::memory::types::normalize_utc(now_utc);

    // 步骤 1–3：到期者优先，口径与 `list_due_memory_units` 一致。
    let due: Option<i64> = match conn.query_row(
        "SELECT id FROM memory_units
          WHERE profile_id = ?1
            AND linked_learning_item_id = ?2
            AND next_review_at IS NOT NULL
            AND next_review_at <= ?3
          ORDER BY next_review_at ASC, id ASC
          LIMIT 1",
        params![profile_id, learning_item_id, now],
        |r| r.get(0),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TrainingError::db(e)),
    };
    if due.is_some() {
        return Ok(due);
    }

    // 步骤 4 + 5：只取前两条即可判定「恰好一个」还是「多个」。
    let mut stmt = conn
        .prepare(
            "SELECT id FROM memory_units
              WHERE profile_id = ?1 AND linked_learning_item_id = ?2
              ORDER BY id ASC
              LIMIT 2",
        )
        .map_err(TrainingError::db)?;
    let ids: Vec<i64> = stmt
        .query_map(params![profile_id, learning_item_id], |r| r.get(0))
        .map_err(TrainingError::db)?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(TrainingError::db)?;

    Ok(match ids.len() {
        1 => Some(ids[0]),
        _ => None,
    })
}

// ============================ §19 create_training_run ============================

/// `create_training_run` 的入参。
///
/// `Debug + Clone` 是给调用方与测试用的：重放/重试语义要求「同一份 payload 再来一次」，
/// 因此这个结构必须可复制、可打印，否则调用方只能手工重建一遍 —— 那正是
/// payload 不一致（`IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD`）的来源。
#[derive(Debug, Clone)]
pub struct CreateTrainingRunParams {
    pub profile_id: i64,
    pub learning_item_id: Option<i64>,
    pub mode: DecisionMode,
    pub plan: TrainingSessionPlan,
    pub now_utc: String,
}

/// GROUNDED LEARNING BRIDGE V1 · P1.1 —— 一个**已在写事务之外**编译好的接地材料。
///
/// # 为什么材料必须在事务之外准备好
///
/// `create_training_run` **不接受**「先建 run、稍后再补快照」的两段式调用：
/// run 一旦 COMMIT，它就已经是 READY/RUNNING 的真实训练；此时再写快照失败，
/// 只会留下一个**静默无接地**的训练，而用户看到的是一个看起来正常的训练。
/// 因此编译（可能包含真实的文档检索）放在事务外，**落库**与块的插入同事务。
#[derive(Debug, Clone)]
pub struct PreparedBlockMaterial {
    /// 计划块与已插入块之间的**唯一**连接键（§11 P1.1 锁定：ordinal 即 join key）。
    pub ordinal: i64,
    /// 该块**计划**使用的协议。必须与已插入块的协议、以及材料自身的
    /// `protocol_id` 完全一致 —— 不一致说明「块说一套、材料说另一套」。
    pub protocol_id: ProtocolId,
    pub material: GroundedTrainingMaterial,
}

/// 校验「准备好的材料」与「计划」严格一致。
///
/// 全部在**事务开始之前**完成（§11 P1.1）：能不进事务就发现的错误，不占写锁，
/// 也不会以「事务中途回滚」的形式掩盖掉可读诊断。
fn validate_prepared_materials(
    plan: &TrainingSessionPlan,
    prepared: &[PreparedBlockMaterial],
) -> Result<(), TrainingError> {
    let mut seen: Vec<i64> = Vec::with_capacity(prepared.len());
    for pm in prepared {
        if seen.contains(&pm.ordinal) {
            return Err(TrainingError::new(
                TrainingErrorCode::PreparedMaterialMismatch,
                format!(
                    "接地材料 ordinal {} 重复 —— ordinal 是块与材料的唯一连接键（§11 P1.1）",
                    pm.ordinal
                ),
            ));
        }
        seen.push(pm.ordinal);

        let block = plan
            .blocks
            .iter()
            .find(|b| b.ordinal == pm.ordinal)
            .ok_or_else(|| {
                TrainingError::new(
                    TrainingErrorCode::PreparedMaterialMismatch,
                    format!("接地材料 ordinal {} 在计划里不存在（§11 P1.1）", pm.ordinal),
                )
            })?;

        // 休息块**不得**带材料：break → material_snapshot_json 保持 NULL（P1.3）。
        if block.is_break {
            return Err(TrainingError::new(
                TrainingErrorCode::PreparedMaterialMismatch,
                format!(
                    "休息块（ordinal {}）不得携带接地材料 —— break 的快照必须保持 NULL（P1.3）",
                    pm.ordinal
                ),
            ));
        }

        // 协议必须**精确**一致。不「就近修正」，不静默替换。
        match block.protocol_id {
            Some(pid) if pid == pm.protocol_id => {}
            other => {
                return Err(TrainingError::new(
                    TrainingErrorCode::PreparedMaterialMismatch,
                    format!(
                        "块 ordinal {} 的计划协议是 {:?}，但准备好的材料是 {} —— 协议必须精确一致（§11 P1.1）",
                        pm.ordinal,
                        other.map(|p| p.as_str()),
                        pm.protocol_id.as_str()
                    ),
                ));
            }
        }

        // 材料自身的 protocol_id 也必须与 join key 一致，否则快照里会写下一个
        // 与所在块不符的协议，读侧将无法判断「这份材料是给哪个协议的」。
        if pm.material.protocol_id != pm.protocol_id.as_str() {
            return Err(TrainingError::new(
                TrainingErrorCode::PreparedMaterialMismatch,
                format!(
                    "接地材料内部的 protocol_id（{}）与 ordinal {} 的协议（{}）不一致（§11 P1.1）",
                    pm.material.protocol_id,
                    pm.ordinal,
                    pm.protocol_id.as_str()
                ),
            ));
        }
    }
    Ok(())
}

/// §19 —— **原子**创建（无接地材料的简写形式）。
///
/// 合成/测试路径可以显式地传入**空**材料集；生产路径
/// （[`crate::training::start::start_training_for_item`]）**永远**先准备好材料
/// 再调用 [`create_training_run_with_materials`]，绝不静默省略准备。
pub fn create_training_run(
    conn: &Connection,
    p: CreateTrainingRunParams,
) -> Result<(TrainingRun, Vec<TrainingBlockRun>), TrainingError> {
    create_training_run_with_materials(conn, p, &[])
}

/// §19 —— **原子**创建，并在**同一事务内**把准备好的接地材料落成块快照。
///
/// ```text
/// BEGIN IMMEDIATE
///   verify profile
///   verify target learning item belongs profile
///   apply existing Active StudySession conflict rule
///   create / bind StudySession
///   insert TrainingRun
///   materialize every TrainingBlockRun
///   for each inserted non-break block with prepared material:
///       save_material_snapshot(...)     <-- EXISTING single serializer/writer
///   consume DIRECT intent when applicable
/// COMMIT
/// ```
///
/// 任一失败 → 回滚，因此不会留下「只有 Session 没有 Run」「Run 没有块」
/// 或「有 Run 但块静默无接地」这类孤儿。
///
/// 这是 §11 P1.1 允许的 shape B：**同一个**事务实现的内部入口，
/// 不是第二套 runtime —— `create_training_run` 只是它以空材料集调用的简写。
/// 之所以 `pub(crate)` 而不是 `pub`，是为了不给生产引入第二个公开入口。
pub(crate) fn create_training_run_with_materials(
    conn: &Connection,
    p: CreateTrainingRunParams,
    prepared: &[PreparedBlockMaterial],
) -> Result<(TrainingRun, Vec<TrainingBlockRun>), TrainingError> {
    if !p.plan.is_executable() {
        return Err(TrainingError::new(
            TrainingErrorCode::PlanHasNoBlocks,
            "计划没有任何学习块，不能创建训练（§19）".to_string(),
        ));
    }

    // 计划里的 ordinal 必须唯一，否则 UNIQUE(training_run_id, ordinal) 会以约束错误暴露。
    // 提前给出可读诊断，并明确这是**计划本身**的问题。
    {
        let mut seen: Vec<i64> = p.plan.blocks.iter().map(|b| b.ordinal).collect();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        if seen.len() != before {
            return Err(TrainingError::new(
                TrainingErrorCode::PlanHasNoBlocks,
                "计划中的块 ordinal 存在重复，无法物化（§10 的 UNIQUE(training_run_id, ordinal)）"
                    .to_string(),
            ));
        }
    }

    // 先做纯校验（§10 不变量），再开事务 —— 能不进事务就发现的错误不必占写锁。
    for block in &p.plan.blocks {
        validate_block_invariant(block.is_break, block.protocol_id, None)?;
    }

    // §11 P1.1：材料与计划的一致性同样在事务**之外**判定。
    validate_prepared_materials(&p.plan, prepared)?;

    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<(TrainingRun, Vec<TrainingBlockRun>), TrainingError> {
        assert_profile_exists(tx, p.profile_id)?;
        if let Some(item_id) = p.learning_item_id {
            assert_item_in_profile(tx, p.profile_id, item_id)?;
        }

        // §8 的唯一开放位：一个档案同时最多一个未终结的 TrainingRun。
        let open_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM training_runs
                  WHERE profile_id = ?1 AND status IN ('ready','active','paused')",
                params![p.profile_id],
                |r| r.get(0),
            )
            .map_err(TrainingError::db)?;
        if open_count > 0 {
            return Err(TrainingError::new(
                TrainingErrorCode::OpenTrainingRunExists,
                "该档案已有一个未结束的训练，请先继续或放弃它（§8 唯一开放位）".to_string(),
            ));
        }

        // ---- 既有 Active StudySession 冲突规则 + create/bind ----
        let study_session_id = resolve_study_session(tx, &p)?;

        // ---- 插入 TrainingRun（初始 ready）----
        let plan_json = serde_json::to_string(&p.plan)
            .map_err(|e| TrainingError::db(format!("计划序列化失败：{e}")))?;
        tx.execute(
            "INSERT INTO training_runs
                 (profile_id, study_session_id, learning_item_id, mode, status,
                  current_block_ordinal, plan_snapshot_json)
             VALUES (?1, ?2, ?3, ?4, 'ready', NULL, ?5)",
            params![
                p.profile_id,
                study_session_id,
                p.learning_item_id,
                p.mode.as_str(),
                plan_json,
            ],
        )
        .map_err(TrainingError::db)?;
        let run_id = tx.last_insert_rowid();

        // ---- 物化每一个块 ----
        for block in &p.plan.blocks {
            // §11：只有「回忆兼容协议 + 有目标学习项」的学习块才尝试绑定。
            let memory_unit_id = match (block.is_break, block.protocol_id, p.learning_item_id) {
                (false, Some(pid), Some(item_id)) if is_recall_compatible(pid) => {
                    resolve_recall_memory_unit(tx, p.profile_id, item_id, &p.now_utc)?
                }
                _ => None,
            };

            // 带上 memory_unit_id 再校验一次：休息块绑定记忆单元会被这里挡住（§10）。
            validate_block_invariant(block.is_break, block.protocol_id, memory_unit_id)?;

            tx.execute(
                "INSERT INTO training_block_runs
                     (profile_id, training_run_id, ordinal, protocol_id, is_break,
                      goal, planned_minutes, memory_unit_id, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending')",
                params![
                    p.profile_id,
                    run_id,
                    block.ordinal,
                    block.protocol_id.map(|pid| pid.as_str()),
                    if block.is_break { 1 } else { 0 },
                    block.goal,
                    block.minutes,
                    memory_unit_id,
                ],
            )
            .map_err(TrainingError::db)?;

            // ---- §11 P1.1 PHASE B：接地快照与块**同事务**落库 ----
            //
            // 用刚插入的块的 id（ordinal 已由 `validate_prepared_materials` 保证唯一且
            // 在计划内，因此这里按 ordinal 定位是精确的）。序列化/写入**只有**
            // `save_material_snapshot` 一处实现 —— 本函数不引入第二个 serializer。
            //
            // 失败 → 直接返回 Err → `finish_immediate` 执行 ROLLBACK，
            // 整次创建（Session 绑定 / Run / 全部块 / DIRECT 意图消费）一起撤销。
            if let Some(pm) = prepared.iter().find(|pm| pm.ordinal == block.ordinal) {
                let block_run_id = tx.last_insert_rowid();
                save_material_snapshot(tx, p.profile_id, block_run_id, &pm.material).map_err(
                    |e| {
                        TrainingError::new(
                            TrainingErrorCode::GroundedSnapshotPersistFailed,
                            format!(
                                "接地材料快照写入失败（block ordinal {}），创建事务整体回滚：{e}",
                                block.ordinal
                            ),
                        )
                    },
                )?;
            }
        }

        // ---- §5：DIRECT 意图在成功创建 TrainingRun 时**同事务**消费 ----
        if p.mode == DecisionMode::Direct {
            clear_active_intent_in_tx(tx, p.profile_id)
                .map_err(|e| TrainingError::db(e.message))?;
        }

        let run = load_run(tx, p.profile_id, run_id)?;
        let blocks = list_block_runs(tx, p.profile_id, run_id)?;
        Ok((run, blocks))
    })();

    finish_immediate(conn, result)
}

/// 既有 Active StudySession 规则的**复用**，而不是第二套规则：
/// 同一档案同一时刻只能有一个 active Session。
///
/// - 已有 active 且**指向同一个学习项** → 绑定它（训练发生在这次学习里）；
/// - 已有 active 但指向**别的**学习项 → 冲突，明确报错（不静默切换，不偷偷新建）；
/// - 没有 active → 新建一个（复用 `StudySessionRepository` 的既有语义）。
fn resolve_study_session(
    tx: &Connection,
    p: &CreateTrainingRunParams,
) -> Result<Option<i64>, TrainingError> {
    let existing: Option<(i64, Option<i64>)> = match tx.query_row(
        "SELECT id, learning_item_id FROM study_sessions
          WHERE profile_id = ?1 AND status = 'active'
          ORDER BY id DESC LIMIT 1",
        params![p.profile_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TrainingError::db(e)),
    };

    if let Some((session_id, session_item)) = existing {
        if session_item == p.learning_item_id {
            return Ok(Some(session_id));
        }
        return Err(TrainingError::new(
            TrainingErrorCode::ActiveSessionConflict,
            "你已有一项学习正在进行，请先继续或结束当前学习。".to_string(),
        ));
    }

    let repo = StudySessionRepository::new(tx);
    let session = match p.learning_item_id {
        Some(item_id) => repo.start_for_item(item_id, None),
        None => repo.start_quick(p.profile_id, None),
    }
    .map_err(TrainingError::db)?;
    Ok(Some(session.id))
}

// ============================ 状态推进 ============================

/// §9：按状态机推进一个 TrainingRun，非法转移返回 typed error。
pub fn transition_training_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
    to: TrainingRunStatus,
) -> Result<TrainingRun, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<TrainingRun, TrainingError> {
        let run = load_run(tx, profile_id, training_run_id)?;
        transition_run_status(run.status, to)?;

        // 首次进入 Active 时记录 started_at；之后不再覆盖（时间事实只写一次）。
        let started_at_sql = if to == TrainingRunStatus::Active && run.started_at.is_none() {
            "datetime('now')"
        } else {
            "started_at"
        };
        tx.execute(
            &format!(
                "UPDATE training_runs
                    SET status = ?1,
                        started_at = {started_at_sql},
                        updated_at = datetime('now')
                  WHERE id = ?2 AND profile_id = ?3"
            ),
            params![to.as_str(), training_run_id, profile_id],
        )
        .map_err(TrainingError::db)?;

        load_run(tx, profile_id, training_run_id)
    })();
    finish_immediate(conn, result)
}

/// HOTFIX-01 FIX D —— 训练的**唯一**初始启动通路。
///
/// ```text
/// BEGIN IMMEDIATE
///   run.status                Ready → Active      （started_at = 真实开始时刻）
///   找 ordinal 最小的 pending 块
///   该块                      Pending → Active    （started_at = 真实开始时刻）
///   run.current_block_ordinal = 该块 ordinal
/// COMMIT
/// ```
///
/// # 为什么不能用 `transition_training_run(..., Active)` 代替
///
/// 通用状态推进**只**改 run 的状态。用它启动训练会留下一个「已经 active、
/// 却没有任何活跃块」的训练：`current_block_ordinal` 仍是 `NULL`，
/// 第一个块仍是 `Pending`，而 FIX C 又禁止 `Pending` 块写入学习事实 ——
/// 结果是一个「已经开始、但什么都做不了」的死状态。
///
/// 初始启动是一个**复合**事实（run 状态 + 块状态 + 当前块指针 + 两个真实时刻），
/// 所以它必须是一个原子操作，而不是让调用方按顺序拼三步。
///
/// # 没有块
///
/// 计划为空 → `TRAINING_RUN_HAS_NO_BLOCKS`，整个事务回滚。
/// 不伪造一个块，也不把 run 留在半启动状态。
pub fn start_training_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingRun, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<TrainingRun, TrainingError> {
        let run = load_run(tx, profile_id, training_run_id)?;
        if run.status.is_terminal() {
            return Err(TrainingError::new(
                TrainingErrorCode::TerminalRunState,
                format!(
                    "训练 {} 已处于终态 {}，不能启动（§9）",
                    run.id,
                    run.status.as_str()
                ),
            ));
        }
        // 只有 Ready 是「初始启动」。Active 再启动一次会重置块计时；
        // Paused 的恢复属于状态推进，不属于启动（FIX D）。
        if run.status != TrainingRunStatus::Ready {
            return Err(TrainingError::new(
                TrainingErrorCode::IllegalRunTransition,
                format!(
                    "训练 {} 的状态是 {}，只有 ready 才能被启动；\
                     暂停后恢复请走状态推进（FIX D）",
                    run.id,
                    run.status.as_str()
                ),
            ));
        }

        // 第一个块 = ordinal 最小的 pending 块。**不猜测**：没有就是没有。
        let first: Option<(i64, i64)> = match tx.query_row(
            "SELECT id, ordinal FROM training_block_runs
              WHERE training_run_id = ?1 AND status = 'pending'
              ORDER BY ordinal ASC
              LIMIT 1",
            params![run.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(TrainingError::db(e)),
        };

        let Some((block_id, ordinal)) = first else {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingRunHasNoBlocks,
                format!("训练 {} 没有任何待进行的块，无法开始（FIX D）", run.id),
            ));
        };

        tx.execute(
            "UPDATE training_runs
                SET status = 'active',
                    started_at = COALESCE(started_at, datetime('now')),
                    current_block_ordinal = ?1,
                    updated_at = datetime('now')
              WHERE id = ?2 AND profile_id = ?3",
            params![ordinal, run.id, profile_id],
        )
        .map_err(TrainingError::db)?;

        // 第一个块的计时从**此刻**开始，而不是从 run 创建那一刻开始。
        tx.execute(
            "UPDATE training_block_runs
                SET status = 'active',
                    started_at = COALESCE(started_at, datetime('now')),
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![block_id, profile_id],
        )
        .map_err(TrainingError::db)?;

        load_run(tx, profile_id, training_run_id)
    })();
    finish_immediate(conn, result)
}

/// §20 —— 完成必须与 StudySession 终结**同事务**。
///
/// `TrainingRun` 只在 `StudySession` 终结成功之后才被标记为 `completed`；
/// 并且**不新建**第二套 Session 完成真相，而是复用既有
/// `StudySessionRepository::end`（其本身已幂等，见其文档）。
pub fn complete_training_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingRun, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<TrainingRun, TrainingError> {
        let run = load_run(tx, profile_id, training_run_id)?;
        transition_run_status(run.status, TrainingRunStatus::Completed)?;

        // ---- HOTFIX-01 FIX F1：还有没走完的块，就不能算完成 ----
        //
        // 三个条件必须同时成立：
        //
        // ```text
        // 没有 pending 块
        // 没有 active 块
        // current_block_ordinal IS NULL
        // ```
        //
        // 第三条不是前两条的推论，而是**独立**要求：它保证「当前块指针」也归零，
        // 否则一个所有块都终结、指针却仍指向某块的 run 会被判成完成 ——
        // 那样下次读它会以为「还有一个当前块」，而那个块其实早已终态。
        //
        // 提前完成会同时虚报两件事：训练做完了、这次学习结束了（§50）。
        let open_blocks: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM training_block_runs
                  WHERE training_run_id = ?1 AND status IN ('pending','active')",
                params![run.id],
                |r| r.get(0),
            )
            .map_err(TrainingError::db)?;
        if open_blocks > 0 || run.current_block_ordinal.is_some() {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingRunHasOpenBlocks,
                format!(
                    "训练 {} 还有 {open_blocks} 个未终结的块（current_block_ordinal={:?}），\
                     不能标记为完成；请先完成或提前结束（FIX F1）",
                    run.id, run.current_block_ordinal
                ),
            ));
        }

        // 先终结 Session：它失败则整个事务回滚，Run 不会被标记完成。
        if let Some(session_id) = run.study_session_id {
            StudySessionRepository::new(tx)
                .end(session_id, None)
                .map_err(TrainingError::db)?;
        }

        tx.execute(
            "UPDATE training_runs
                SET status = 'completed',
                    ended_at = datetime('now'),
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![training_run_id, profile_id],
        )
        .map_err(TrainingError::db)?;

        load_run(tx, profile_id, training_run_id)
    })();
    finish_immediate(conn, result)
}

/// HOTFIX-01 FIX F2 —— 提前结束训练。
///
/// ```text
/// BEGIN IMMEDIATE
///   run.status ∈ Ready / Active / Paused
///   所有剩余 Pending / Active 块 → Skipped
///   终结 StudySession
///   run.status = Abandoned
///   current_block_ordinal = NULL
///   ended_at = 真实结束时刻
/// COMMIT
/// ```
///
/// # 为什么必须是一个原子操作
///
/// 半终结是最糟的中间态：run 已经 `abandoned` 而 Session 还 `active`，
/// 或者块还是 `active` 而 run 已终结 —— 前者会让「唯一 active Session」的
/// 既有规则一直挡着用户开新学习，后者会让一个终态 run 里留着活跃块。
/// 所以四件事必须同生共死。
///
/// # 它**不**做什么
///
/// ```text
/// 不写任何成功证据        不推进 FSRS
/// 不写 ErrorCorrected     不把「停止」写成「失败」
/// ```
///
/// 用户的「我不学了」是一条**关于时间的事实**，不是一条关于学会了什么的事实（§50）。
pub fn abandon_training_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingRun, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<TrainingRun, TrainingError> {
        let run = load_run(tx, profile_id, training_run_id)?;
        transition_run_status(run.status, TrainingRunStatus::Abandoned)?;

        // 剩余块一律记 `Skipped` —— 刻意**不**记 `Completed`：
        // 「跳过」与「完成」是两种不同的真相（§50），把提前结束写成完成
        // 等于虚报了一次完整训练。
        tx.execute(
            "UPDATE training_block_runs
                SET status = 'skipped',
                    ended_at = COALESCE(ended_at, datetime('now')),
                    updated_at = datetime('now')
              WHERE training_run_id = ?1 AND profile_id = ?2
                AND status IN ('pending','active')",
            params![run.id, profile_id],
        )
        .map_err(TrainingError::db)?;

        if let Some(session_id) = run.study_session_id {
            StudySessionRepository::new(tx)
                .end(session_id, None)
                .map_err(TrainingError::db)?;
        }

        tx.execute(
            "UPDATE training_runs
                SET status = 'abandoned',
                    current_block_ordinal = NULL,
                    ended_at = datetime('now'),
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![training_run_id, profile_id],
        )
        .map_err(TrainingError::db)?;

        load_run(tx, profile_id, training_run_id)
    })();
    finish_immediate(conn, result)
}

// ============================ §13–§15 交互的恰好一次管线 ============================

/// `record_interaction` 的入参。
///
/// §13 要求「网络重试必须复用同一个 payload」。让这个结构可 `Clone`，
/// 就是把「原样再来一次」变成一件调用方做得到、且不会写错的事。
#[derive(Debug, Clone)]
pub struct RecordInteractionParams {
    pub profile_id: i64,
    pub training_run_id: i64,
    pub block_run_id: i64,
    /// §13：由前端为**用户动作**生成一次；网络重试必须复用同一个值。
    pub client_action_id: String,
    pub interaction_type: String,
    pub prompt_text: Option<String>,
    pub user_response_text: Option<String>,
    pub hint_level: Option<i64>,
    pub result: Option<InteractionResult>,
    /// §21 的判定方式；决定本次证据质量上限（§22）**与权威性**（HOTFIX-01 FIX A）。
    ///
    /// 手工前端提交永远只能拿到 `SelfCheck`（FIX A1）—— 由命令层决定，
    /// 前端没有参数可以把它调高。
    pub verification: VerificationMethod,
    /// 本次事实的发生时间。`None` = 由领域层取当前 UTC（§18）。
    ///
    /// 这里刻意是 `Option`：时钟属于领域层，不属于传输层。若让命令层负责填默认值，
    /// 每个新增调用方都要重新猜一次格式，而格式错了不会报错、只会静默污染排序。
    pub occurred_at: Option<String>,
}

/// 一次交互的返回值：既有行 + 效果摘要。
///
/// `replayed = true` 表示这是**重试命中幂等键**，没有产生任何新事实。
///
/// `Serialize` 是给 IPC 用的：前端需要看到 `effect` 才能诚实呈现
/// 「这次有没有推进 FSRS / 为什么没有」，而不是自己猜。
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct InteractionOutcome {
    pub interaction: TrainingInteraction,
    pub effect: EffectSummary,
    pub replayed: bool,
}

/// §15 —— 恰好一次事实管线。
///
/// ```text
/// BEGIN IMMEDIATE
///   check idempotency
///   insert training_interaction
///   record deterministic LearningMoment
///   if trusted recall result AND block has memory_unit_id: apply FSRS exactly once
///   store effect_summary_json
/// COMMIT
/// ```
///
/// 任何 DB 失败 → 整体回滚。**不允许**出现「有 interaction 没有 moment」
/// 或「有 moment 但 FSRS 只更新一半」的状态。
pub fn record_interaction(
    conn: &Connection,
    p: RecordInteractionParams,
) -> Result<InteractionOutcome, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<InteractionOutcome, TrainingError> {
        assert_profile_exists(tx, p.profile_id)?;

        // ---- §14：先查幂等键 ----
        if let Some(existing) =
            find_interaction_by_action_id(tx, p.profile_id, &p.client_action_id)?
        {
            return handle_duplicate(existing, &p);
        }

        // §18：本次事实的**发生时间**。调用方可以不传；一旦不传，由领域层用与全库
        // 完全一致的 UTC 文本格式（`YYYY-MM-DD HH:MM:SS`）落定。
        //
        // 为什么默认值必须在这里、而不是 IPC 层：`occurred_at` 会被 SQLite 当作
        // **纯字符串**参与比较，并交给 `date()` / `datetime()` 解析 —— 见
        // `learning_moment` 的时间窗查询、`ai/context.rs` 的 `date(occurred_at, '+8 hours')`、
        // `memory/engine.rs` 的 `days_between`。格式一旦不统一，`'T'`(0x54) 与
        // `' '`(0x20) 的差值会让同一天的记录排序错位，`date()` 的解析语义也会漂移。
        // 那是一种不报错、只按时间累积的真相污染，所以格式必须只有一个来源。
        let occurred_at = p
            .occurred_at
            .clone()
            .unwrap_or_else(crate::cognitive::today_projection::utc_now);

        // 归属校验：run 与 block 都必须属于该档案，且 block 属于该 run。
        let run = load_run(tx, p.profile_id, p.training_run_id)?;
        let block = load_block_run(tx, p.profile_id, p.block_run_id)?;
        if block.training_run_id != run.id {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockNotFound,
                format!(
                    "块 {} 不属于训练 {}（跨 run 引用被拒绝）",
                    p.block_run_id, p.training_run_id
                ),
            ));
        }
        // 终态训练不接受新交互。
        if run.status.is_terminal() {
            return Err(TrainingError::new(
                TrainingErrorCode::TerminalRunState,
                format!(
                    "训练 {} 已处于终态 {}，不再接受交互（§9）",
                    run.id,
                    run.status.as_str()
                ),
            ));
        }

        // ---- HOTFIX-01 FIX C：只有「当前活跃块」才允许写入学习事实 ----
        //
        // 为什么必须在写 interaction **之前**：interaction 行本身也是事实。
        // 若先写行再判，一个 pending 块就会留下「用户确实提交过」的痕迹 ——
        // 而那次提交发生在一次**尚未开始**（或已经结束）的学习里，
        // 那是一条本不该存在的事实。所以判定必须发生在第一个 INSERT 之前。
        //
        // 结果：没有 TrainingInteraction、没有 LearningMoment、没有 Evidence、
        // 没有 MemoryReview、没有 FSRS —— 整个事务在写任何东西之前就结束了。
        if !block_is_current_active(&run, &block) {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockNotCurrentActive,
                format!(
                    "块 {} 不是当前进行中的块（run.status={}, block.status={}, \
                     block.ordinal={}, current_block_ordinal={:?}）；\
                     只有当前活跃块才能写入学习事实（HOTFIX-01 FIX C）",
                    block.id,
                    run.status.as_str(),
                    block.status.as_str(),
                    block.ordinal,
                    run.current_block_ordinal,
                ),
            ));
        }

        // ---- 写入 interaction ----
        tx.execute(
            "INSERT INTO training_interactions
                 (profile_id, training_run_id, block_run_id, client_action_id, interaction_type,
                  prompt_text, user_response_text, hint_level, result, effect_summary_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, '{}')",
            params![
                p.profile_id,
                p.training_run_id,
                p.block_run_id,
                p.client_action_id,
                p.interaction_type,
                p.prompt_text,
                p.user_response_text,
                p.hint_level,
                p.result.map(|r| r.as_str()),
            ],
        )
        .map_err(TrainingError::db)?;
        let interaction_id = tx.last_insert_rowid();

        // ---- §10 / §16 / item 10（owner 收口决定）：休息块不产生任何学习事实 ----
        //
        // 休息块（§10：`protocol_id IS NULL` + `is_break = 1`）**只**写交互行作为审计轨迹；
        // 它**绝不**变成 LearningMoment、Evidence 或 FSRS 推进。这一点过去被 §6.4 标记为
        // 「Owner 待定」，现在 owner 已明确：休息块 = 零学习事实。
        //
        // 注意：交互行本身仍是合法的审计轨迹（谁、什么时间、做了什么动作），但本模块
        // 不再把它推导成任何 LearningMoment —— 否则 break 会污染 mastery / FSRS（§50）。
        if block.is_break {
            let effect = EffectSummary {
                learning_moment_ids: Vec::new(),
                fsrs_applied: false,
                memory_review_id: None,
                memory_unit_id: None,
                fsrs_skip_reason: Some(FSRS_SKIP_BLOCK_IS_BREAK.to_string()),
                verification: p.verification.as_str().to_string(),
            };
            let effect_json = serde_json::to_string(&effect)
                .map_err(|e| TrainingError::db(format!("效果摘要序列化失败：{e}")))?;
            tx.execute(
                "UPDATE training_interactions SET effect_summary_json = ?1 WHERE id = ?2",
                params![effect_json, interaction_id],
            )
            .map_err(TrainingError::db)?;
            let interaction = load_interaction(tx, p.profile_id, interaction_id)?;
            return Ok(InteractionOutcome {
                interaction,
                effect,
                replayed: false,
            });
        }

        // ---- §18：记录 LearningMoment（来源可溯源到这次交互）----
        //
        // HOTFIX-01 FIX B：类型由**后端**从 (ProtocolId, interaction_type, result,
        // verification) 推导 —— 调用方连一个可以声明的字段都没有了。
        // 推导结果可能是 `None`（FIX B7：不存在诚实的类型），那时只落交互行。
        let evidence_quality = p.verification.max_evidence_quality();
        let source_type = source_type_for(p.verification);
        let moment_type = derive_moment_type(
            block.protocol_id,
            &p.interaction_type,
            p.result,
            p.verification,
        )
        // 第二道防线（FIX A3）：即便推导逻辑将来被改错，非权威判定也**绝不**
        // 可能写出「成功」类事实。这一层保护的是数据库写入路径本身，
        // 而不是某一个调用方。
        .map(|declared| enforce_authority(declared, p.verification));

        let mut effect = EffectSummary {
            learning_moment_ids: Vec::new(),
            fsrs_applied: false,
            memory_review_id: None,
            memory_unit_id: block.memory_unit_id,
            fsrs_skip_reason: None,
            verification: p.verification.as_str().to_string(),
        };

        let recorded = match moment_type {
            Some(moment_type) => {
                let mut moment = NewLearningMoment::new(
                    p.profile_id,
                    moment_type,
                    occurred_at.clone(),
                    source_type,
                    evidence_quality,
                );
                moment.session_id = run.study_session_id;
                moment.learning_item_id = run.learning_item_id;
                moment.hint_level = p.hint_level;
                // 把调用方声明的 result 原样交给既有的 `validate_new_moment`：
                // 若它与 moment_type 的内在结果矛盾，写入会失败并整体回滚 —— 这是刻意的，
                // 宁可拒绝一条自相矛盾的证据，也不要落库一条无法解释的 moment。
                moment.result = p.result.map(|r| r.as_str().to_string());
                moment.source_id = Some(training_source_id(interaction_id));
                moment.metadata_json = serde_json::json!({
                    "provenance": {
                        "training_run_id": p.training_run_id,
                        "block_run_id": p.block_run_id,
                        "interaction_id": interaction_id,
                    },
                    "verification": p.verification.as_str(),
                    "interaction_type": p.interaction_type,
                });
                let recorded = record_learning_moment(tx, moment)
                    .map_err(|e| TrainingError::db(format!("LearningMoment 写入失败：{e}")))?;
                effect.learning_moment_ids = vec![recorded.id];
                Some(recorded)
            }
            // FIX B7：这次交互没有诚实的 moment 类型 —— 交互行已经落库，
            // 完成规则会去读它（D16），但**不**签发任何学习证据。
            None => None,
        };

        // ---- §15 + §11 + FIX A3：可信回忆结果 + 已绑定记忆单元 → 恰好一次推进 FSRS ----
        // 休息块的路径已在上面提前返回（见 item 10），这里只会到达学习块。
        if !p.verification.is_authoritative() {
            // FIX A3：权威性是 `VerificationMethod` **自身**的属性，在这里集中判一次。
            // 自检（`SelfCheck`）与 AI（`AiTutor`）都不是可授权的学习事实：
            // 无论证据质量看起来多高、moment 类型看起来多像结果，都不得移动 FSRS。
            // 这条判定刻意放在「是否回忆类 moment」之前 —— 否则非权威结果会被
            // 笼统归因为「不是回忆结果」，掩盖真正的原因。
            effect.fsrs_skip_reason = Some(FSRS_SKIP_NON_AUTHORITATIVE.to_string());
        } else if moment_type.is_none() {
            // FIX B7：没有推导出任何诚实的 moment → 没有任何东西可以推进排程。
            // 明确说出「没有发生」，而不是沉默（§50）。
            effect.fsrs_skip_reason = Some(FSRS_SKIP_NO_MOMENT.to_string());
        } else if !is_recall_moment(moment_type.expect("上一臂已排除 None")) {
            effect.fsrs_skip_reason = Some(FSRS_SKIP_NOT_RECALL_MOMENT.to_string());
        } else if block.memory_unit_id.is_none() {
            // §11：未绑定 → 不推进。这是 unknown/unbound，**不是失败**。
            effect.fsrs_skip_reason = Some(FSRS_SKIP_NO_MEMORY_UNIT.to_string());
        } else if !evidence_quality.is_trusted() {
            effect.fsrs_skip_reason = Some(FSRS_SKIP_EVIDENCE_TOO_LOW.to_string());
        } else {
            let unit_id = block.memory_unit_id.expect("上面已判空");
            let recorded = recorded.expect("权威 + 回忆类 moment 必然已经写入");
            // §16：`_in_tx` 内部先查该 moment 是否已推进过。这里是**同事务**调用，
            // 因此 interaction / moment / review / unit 更新共同构成一个原子事实。
            let review = record_review_from_moment_in_tx(tx, p.profile_id, unit_id, &recorded)
                .map_err(|e| TrainingError::db(format!("FSRS 推进失败：{e}")))?;
            effect.fsrs_applied = true;
            effect.memory_review_id = Some(review.id);
        }

        // ---- 回写 effect_summary_json（同一个事务内）----
        let effect_json = serde_json::to_string(&effect)
            .map_err(|e| TrainingError::db(format!("效果摘要序列化失败：{e}")))?;
        tx.execute(
            "UPDATE training_interactions SET effect_summary_json = ?1 WHERE id = ?2",
            params![effect_json, interaction_id],
        )
        .map_err(TrainingError::db)?;

        let interaction = load_interaction(tx, p.profile_id, interaction_id)?;
        Ok(InteractionOutcome {
            interaction,
            effect,
            replayed: false,
        })
    })();
    finish_immediate(conn, result)
}

/// §14：幂等键已存在时的判定。
///
/// 「同一个动作」= `training_interactions` 上承载**这次动作语义**的全部列都逐字相同。
/// 一致 → 视为重试：原样返回既有结果，不产生新 interaction、不产生新 LearningMoment、
/// 不产生新 Evidence、不推进 FSRS。
///
/// 任何一个不一致 → 同一个键被换成了不同 payload，这是客户端 bug，
/// 必须显式报错而**不是**覆盖原始动作。
///
/// # P5 真值审计：`result` 与 `prompt_text` 曾经不在比较里
///
/// 这不是两个可选的元数据列：
///
/// ```text
/// result      -> derive_moment_type(...) 决定这次交互变成哪一种学习事实
///                Recall 族：Success->RecallSuccess / Partial->RecallPartial
///                           Failure->RecallFailure / 其余->RecallAttempt
///                且只有 Recall* 会推进 FSRS（档位由 result 决定）
/// prompt_text -> 原样落库、被读回呈现的「当时问的是什么」
/// ```
///
/// 少了 `result`，同一个键把 `Success` 换成 `Failure` 会得到 `replayed: true`
/// 与一份「当时是成功」的摘要 —— 调用方声明的结果被静默丢弃，而 API 却声称
/// 这就是它刚才那次动作的真值（§50）。少了 `prompt_text`，换一道题重发
/// 会拿回旧题面的回执。两者都是「把两件不同的事说成一件」。
///
/// 刻意**不**比较 `occurred_at`：它属于领域层的时钟（`None` = 由领域层取当前 UTC），
/// 一次真实重试的到达时间本就不同，比较它会把所有重试都判成冲突。
/// `verification` 也不在这里比较 —— 它由命令层决定（FIX A1），前端没有参数能改它，
/// 且它落在 `effect_summary_json` 而不是独立列上。
fn handle_duplicate(
    existing: TrainingInteraction,
    p: &RecordInteractionParams,
) -> Result<InteractionOutcome, TrainingError> {
    let same = existing.training_run_id == p.training_run_id
        && existing.block_run_id == p.block_run_id
        && existing.interaction_type == p.interaction_type
        && existing.prompt_text == p.prompt_text
        && existing.user_response_text == p.user_response_text
        && existing.hint_level == p.hint_level
        && existing.result == p.result;

    if !same {
        // 诊断必须**点名**到底哪个字段不一致。只写 run/block/type 会让
        // 「换了 result」这种冲突被读成「看起来哪儿都没变」——
        // 这条错误存在的意义正是让客户端 bug 立刻可见。
        let mut diffs: Vec<&str> = Vec::new();
        if existing.training_run_id != p.training_run_id {
            diffs.push("training_run_id");
        }
        if existing.block_run_id != p.block_run_id {
            diffs.push("block_run_id");
        }
        if existing.interaction_type != p.interaction_type {
            diffs.push("interaction_type");
        }
        if existing.prompt_text != p.prompt_text {
            diffs.push("prompt_text");
        }
        if existing.user_response_text != p.user_response_text {
            diffs.push("user_response_text");
        }
        if existing.hint_level != p.hint_level {
            diffs.push("hint_level");
        }
        if existing.result != p.result {
            diffs.push("result");
        }
        return Err(TrainingError::new(
            TrainingErrorCode::IdempotencyKeyReusedWithDifferentPayload,
            format!(
                "幂等键 {} 已被另一个动作使用；不一致的字段：{}（原 run={} block={} type={}，\
                 新 run={} block={} type={}）；重试必须复用完全相同的 payload（§14）",
                p.client_action_id,
                diffs.join(", "),
                existing.training_run_id,
                existing.block_run_id,
                existing.interaction_type,
                p.training_run_id,
                p.block_run_id,
                p.interaction_type,
            ),
        ));
    }

    // §15：重放必须返回**当时真实发生的事**。
    //
    // 这里刻意**不**用 `unwrap_or_default()`：一份空的 `EffectSummary` 会报告
    // `fsrs_applied: false`、没有 moment、没有跳过原因 —— 也就是把
    // 「当时确实推进了 FSRS」谎报成「什么都没发生」。那比直接报错更糟，
    // 因为调用方无法区分「真的没发生」和「读不出来」（§50）。
    //
    // 注意：返回错误**不会**重新执行这次动作，因此不会产生第二个学习事实 ——
    // 幂等键的保护依然成立，只是我们拒绝编造一份摘要。
    let effect: EffectSummary =
        serde_json::from_str(&existing.effect_summary_json).map_err(|e| {
            TrainingError::new(
                TrainingErrorCode::EffectSummaryUnreadable,
                format!(
                    "交互 {} 已存在（幂等键 {}），但它的效果摘要无法解析：{e}。\
                 拒绝用一份空摘要顶替 —— 那会把「确实发生过」谎报成「没发生」。",
                    existing.id, p.client_action_id
                ),
            )
        })?;

    Ok(InteractionOutcome {
        interaction: existing,
        effect,
        replayed: true,
    })
}

/// §21 的判定方式 → moment 来源类型。
///
/// 确定性路径记为 `SystemDerived`；用户自检记为 `UserExplicit`；
/// AI Tutor 记为 `TutorObserved` —— 后者在 `max_quality_for_source` 里上限就是 Medium，
/// 与 §22 一致，构成**第二道**防线（第一道是 `VerificationMethod::max_evidence_quality`）。
fn source_type_for(v: VerificationMethod) -> MomentSourceType {
    match v {
        VerificationMethod::Deterministic | VerificationMethod::Structured => {
            MomentSourceType::SystemDerived
        }
        VerificationMethod::SelfCheck => MomentSourceType::UserExplicit,
        VerificationMethod::AiTutor => MomentSourceType::TutorObserved,
    }
}

/// §22 / §50 / HOTFIX-01 FIX A3 —— **第二道防线**：非权威判定不得写出「结果」类事实。
///
/// # 为什么是 `verification` 而不是 `source_type`
///
/// 旧版本按 `source_type.is_non_authoritative()` 判（只覆盖 `TutorObserved` /
/// `Imported`），于是 `SelfCheck` → `UserExplicit` 被视为权威来源，自检成功
/// 可以一路写成 `RecallSuccess`。HOTFIX-01 FIX A2 把权威性收敛到
/// `VerificationMethod::is_authoritative()`，这一层必须跟着**同一个判据**，
/// 否则两道防线会各判各的，而「权威」就又有了第二个定义。
///
/// # 它做什么
///
/// ```text
/// 权威（Deterministic / Structured） → 原样返回
/// 非权威（SelfCheck / AiTutor）      → 成功/部分/失败一律降级为 attempt
/// ```
///
/// 信息没有丢失：调用方声明的 `result` 仍原样保存在 moment 的 `result` 字段
/// 与 `training_interactions.result` 里。降级的是**断言强度**，不是数据。
///
/// 这是**防御性后备**：`derive_moment_type` 已经做了同样的事。保留这一层，
/// 是因为它保护的是**数据库写入路径本身**，而不是某一个调用方 ——
/// 将来任何新的写路径都无法绕过它。
fn enforce_authority(
    declared: LearningMomentType,
    verification: VerificationMethod,
) -> LearningMomentType {
    if verification.is_authoritative() {
        return declared;
    }
    match declared {
        // 回忆类：成功 / 部分 / 失败 一律降级为 attempt。
        LearningMomentType::RecallSuccess
        | LearningMomentType::RecallPartial
        | LearningMomentType::RecallFailure => LearningMomentType::RecallAttempt,

        LearningMomentType::ExplanationSuccess => LearningMomentType::ExplanationAttempt,

        LearningMomentType::PracticeSuccess | LearningMomentType::PracticeFailure => {
            LearningMomentType::PracticeAttempt
        }

        LearningMomentType::TransferSuccess | LearningMomentType::TransferFailure => {
            LearningMomentType::TransferAttempt
        }

        // `ErrorCorrected` 是「真实修正」的断言，非权威判定不得签发它。
        // 它没有 attempt 形态，因此降级为 `ErrorDetected` —— 这是**保守**方向：
        // 「发现过错误」为真，而「已修正」不再被声称。
        LearningMomentType::ErrorCorrected => LearningMomentType::ErrorDetected,

        // 其余类型本身就不是「权威结果」（HintRequested / QuestionAsked /
        // InterestSignal / ManualNote …），无需降级。
        other => other,
    }
}

// ============================ 块推进（PACK A 收口 · D11–D21）============================
//
// # 这一段存在的唯一理由
//
// W4 决策 D4 记录了一个缺口：`training_block_runs.status` 与
// `training_runs.current_block_ordinal` 在创建之后**从未被写过**，
// 因为当时没有任何东西评估 `CompletionRuleKind`，而两个候选猜测
// （一律 `completed` / 一律 `skipped`）都是编造。
//
// Owner 补充决定 D11–D21 把这个缺口收口了。这一段的全部纪律可以写成三行：
//
// ```text
// BLOCK COMPLETED  !=  LEARNING MASTERED
// USER FINISHED    !=  USER SUCCEEDED
// TIME SPENT       !=  LEARNING EVIDENCE
// ```
//
// 因此这里**只写**块状态与 `current_block_ordinal`。它不写：
//
// ```text
// learning_moments   memory_reviews   memory_units   mastery
// ```
//
// # D19 —— 通用兜底同样不许绕过求值器
//
// 没有专属体验的 ProtocolId **保留它自己的** `protocol_id` 与
// `completion_rule`，并走**同一个**后端求值器。前端可以渲染通用控件，
// 但绝不能自己判定「看起来做完了」。

/// 一个块的完成契约状态（只读投影，供 UI 显形）。
///
/// 前端拿到它之后**仍然**不能自行判定完成：它只用来把后端已经写死的
/// 冻结规则讲给用户听，以及解释「为什么现在还不能往下走」。
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct BlockCompletionState {
    pub block_run_id: i64,
    /// 该块使用的冻结完成规则（D15）。
    pub rule_kind: CompletionRuleKind,
    /// 冻结规则的人话描述（`CompletionRule.description_zh`）。
    pub rule_zh: String,
    /// 按**当前**已落库事实，完成契约是否已满足。
    pub satisfied: bool,
    /// 稳定原因码（永不为 `None` / 空串）。
    pub reason: String,
}

/// 一次块推进的结果。
///
/// `learning_moment_ids` / `fsrs_applied` 被**刻意**保留在这份返回值里，
/// 而且恒为空 / 恒为 false：它们不是「顺便带上的字段」，而是把 D11 的
/// 约束变成**调用方和前端都能看见**的事实 —— 每次推进都必须能回答
/// 「这次有没有产生学习证据」，答案是「没有」，并且要能被测到。
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct BlockAdvanceOutcome {
    pub run: TrainingRun,
    pub block: TrainingBlockRun,
    /// 凭什么可以往下走（未推进则为 `None`）。
    pub progression: Option<BlockProgression>,
    /// 稳定原因码。
    pub reason: String,
    /// 是否真的落库推进了。`false` = 规则未满足，**什么都没写**。
    pub advanced: bool,
    /// 下一个应该进入的块（没有剩余块则为 `None`）。
    pub next_block_id: Option<i64>,
    /// **恒为空**（D11）：块推进从不产生 LearningMoment。
    pub learning_moment_ids: Vec<i64>,
    /// **恒为 false**（D11 / D18）：块推进从不推进 FSRS。
    pub fsrs_applied: bool,
}

/// 用户权威推进一个块（D12 `Finish` / D13 `Stop`）。
#[derive(Debug, Clone)]
pub struct AdvanceBlockParams {
    pub profile_id: i64,
    pub training_run_id: i64,
    pub block_run_id: i64,
    /// 用户权威的形态。**两种都允许往下走**，但语义后果不同（见模块头）。
    pub intent: BlockAdvanceIntent,
    /// 时间片规则用的已用分钟数（D17 块计时状态）。`None` = 未知。
    pub elapsed_minutes: Option<i64>,
}

/// 纯规则推进：只有冻结完成规则被满足时才往前走。
#[derive(Debug, Clone)]
pub struct TryCompleteBlockParams {
    pub profile_id: i64,
    pub training_run_id: i64,
    pub block_run_id: i64,
    pub elapsed_minutes: Option<i64>,
}

/// 该块的冻结完成规则。**绝不猜测**（D19）。
///
/// - 休息块（§10：`protocol_id IS NULL`）没有协议 → 用时间片语义；
/// - 学习块 → 用注册表里该协议**自己的** `completion_rule`，
///   通用兜底也保留原 `protocol_id`，不替换成别的规则（D19）。
fn completion_rule_for(block: &TrainingBlockRun) -> Result<CompletionRuleKind, TrainingError> {
    match block.protocol_id {
        Some(pid) => Ok(find_protocol(pid).completion_rule.kind),
        None if block.is_break => Ok(CompletionRuleKind::TimeSliceOrUserStop),
        None => Err(TrainingError::new(
            TrainingErrorCode::BreakBlockInvariantViolated,
            format!(
                "块 {} 既没有 protocol_id 也不是休息块，无法判定完成（§10）",
                block.id
            ),
        )),
    }
}

/// 从**落库事实**里收集完成判定所需的全部输入（D17）。
///
/// 只读 `training_interactions` 与由这些交互合法产生的 `learning_moments`，
/// 全部限定在同一 profile / 同一 run / 同一 block 内。
fn gather_completion_facts(
    tx: &Connection,
    profile_id: i64,
    block: &TrainingBlockRun,
    elapsed_minutes: Option<i64>,
    explicit_finish: bool,
    user_stop: bool,
) -> Result<CompletionFacts, TrainingError> {
    let mut stmt = tx
        .prepare(
            "SELECT id, interaction_type, result
               FROM training_interactions
              WHERE profile_id = ?1 AND block_run_id = ?2
              ORDER BY id ASC",
        )
        .map_err(TrainingError::db)?;
    let rows: Vec<(i64, String, Option<InteractionResult>)> = stmt
        .query_map(params![profile_id, block.id], |r| {
            let result_raw: Option<String> = r.get(2)?;
            Ok((
                r.get(0)?,
                r.get(1)?,
                result_raw.as_deref().and_then(InteractionResult::parse),
            ))
        })
        .map_err(TrainingError::db)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(TrainingError::db)?;

    let interactions: Vec<BlockInteractionFact> = rows
        .iter()
        .map(|(_, interaction_type, result)| BlockInteractionFact {
            interaction_type: interaction_type.clone(),
            result: *result,
        })
        .collect();

    Ok(CompletionFacts {
        moment_types: load_moment_types_for_interactions(
            tx,
            profile_id,
            &rows.iter().map(|(id, _, _)| *id).collect::<Vec<i64>>(),
        )?,
        interactions,
        planned_minutes: block.planned_minutes,
        elapsed_minutes,
        explicit_finish,
        user_stop,
    })
}

/// 这些交互**合法产生**的 LearningMoment 类型。
///
/// 走的是 §18 的 `idx_learning_moments_training_source`（profile_id, source_id），
/// 即只认 `training_interaction:<id>` 这条溯源链 —— 别的来源的 moment
/// 不是这个块的行为，不能拿来判定这个块完成（D17）。
fn load_moment_types_for_interactions(
    tx: &Connection,
    profile_id: i64,
    interaction_ids: &[i64],
) -> Result<Vec<LearningMomentType>, TrainingError> {
    if interaction_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sources = interaction_ids
        .iter()
        .map(|id| format!("'{}'", training_source_id(*id)))
        .collect::<Vec<String>>()
        .join(",");
    let sql = format!(
        "SELECT moment_type FROM learning_moments
          WHERE profile_id = ?1 AND source_id IN ({sources})"
    );
    let mut stmt = tx.prepare(&sql).map_err(TrainingError::db)?;
    let raw: Vec<String> = stmt
        .query_map(params![profile_id], |r| r.get(0))
        .map_err(TrainingError::db)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(TrainingError::db)?;

    // 未知 moment_type → 明确报错，而不是静默丢弃。
    // 静默丢弃会让「其实发生过一次成功」被判成「什么都没发生」（§50）。
    raw.iter()
        .map(|s| {
            LearningMomentType::parse(s).ok_or_else(|| {
                TrainingError::new(
                    TrainingErrorCode::Db,
                    format!("learning_moments.moment_type 无法解析：{s}"),
                )
            })
        })
        .collect()
}

/// 推进性质 → 块终态。
///
/// ```text
/// RuleSatisfied     规则被真实交互结果满足      → Completed
/// TimeSliceElapsed  时间片确实走完了（D18）     → Completed
/// UserFinished      用户显式「做完了」           → Completed
/// UserStopped       用户「停下 / 跳过」          → Skipped
/// ```
///
/// `UserStopped` 走 `Skipped`，这是 D13 要求的「复用既有 skip/stop 语义」；
/// 而 §50 同时保证 `Skipped` **不是**失败 —— 它只是「这一段结束了，
/// 且没有可核实的完成证据」。
fn terminal_status_for(progression: BlockProgression) -> TrainingBlockStatus {
    match progression {
        BlockProgression::RuleSatisfied
        | BlockProgression::TimeSliceElapsed
        | BlockProgression::UserFinished => TrainingBlockStatus::Completed,
        BlockProgression::UserStopped => TrainingBlockStatus::Skipped,
    }
}

/// 写入块终态 + 推进 `current_block_ordinal`。**只写这两件事**。
fn apply_block_terminal(
    tx: &Connection,
    run: &TrainingRun,
    block: &TrainingBlockRun,
    to: TrainingBlockStatus,
) -> Result<(TrainingBlockRun, Option<i64>), TrainingError> {
    transition_block_status(block.status, to)?;

    tx.execute(
        "UPDATE training_block_runs
            SET status = ?1,
                ended_at = datetime('now'),
                updated_at = datetime('now')
          WHERE id = ?2 AND profile_id = ?3",
        params![to.as_str(), block.id, run.profile_id],
    )
    .map_err(TrainingError::db)?;

    // 下一个待进入的块：ordinal 最小的 pending 块。**不自动激活** ——
    // 激活是用户/前端的显式动作，而 `idx_training_blocks_one_active`
    // 保证一次至多一个 active 块。
    let next: Option<(i64, i64)> = match tx.query_row(
        "SELECT id, ordinal FROM training_block_runs
          WHERE training_run_id = ?1 AND status = 'pending'
          ORDER BY ordinal ASC
          LIMIT 1",
        params![run.id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ) {
        Ok(v) => Some(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(TrainingError::db(e)),
    };

    tx.execute(
        "UPDATE training_runs
            SET current_block_ordinal = ?1,
                updated_at = datetime('now')
          WHERE id = ?2 AND profile_id = ?3",
        params![next.map(|(_, ordinal)| ordinal), run.id, run.profile_id],
    )
    .map_err(TrainingError::db)?;

    let updated = load_block_run(tx, run.profile_id, block.id)?;
    Ok((updated, next.map(|(id, _)| id)))
}

/// 激活一个块：写入 `started_at`（D17 的「块计时状态」）并把它记为当前块。
///
/// 为什么这条通路必须存在：时间片规则需要 `started_at` 才算得出「过了多久」。
/// 没有它，「块计时状态」这一项 D17 允许的输入就是空的，
/// 时间片规则将永远无法满足。
pub fn start_training_block(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
) -> Result<TrainingBlockRun, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<TrainingBlockRun, TrainingError> {
        let run = load_run(tx, profile_id, training_run_id)?;
        if run.status.is_terminal() {
            return Err(TrainingError::new(
                TrainingErrorCode::TerminalRunState,
                format!("训练 {} 已处于终态，不能再激活块（§9）", run.id),
            ));
        }
        let block = load_block_run(tx, profile_id, block_run_id)?;
        if block.training_run_id != run.id {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockNotFound,
                format!(
                    "块 {} 不属于训练 {}（跨 run 引用被拒绝）",
                    block_run_id, training_run_id
                ),
            ));
        }

        // ---- HOTFIX-01 FIX E：按序激活，且只激活「当前」块 ----
        //
        // 四个条件必须同时成立：
        //
        // ```text
        // run.status               == Active      （Ready / Paused 都不行）
        // block.status             == Pending     （终态块 / 已活跃块都不行）
        // block.ordinal            == run.current_block_ordinal
        // 同 run 内没有别的 active 块
        // ```
        //
        // 第三条是 FIX E 的核心：块终结后 `current_block_ordinal` 指向**下一个
        // pending 块**，而那个块此刻是「当前但未激活」。用户必须显式开始它，
        // 时间才从那一刻起算 —— 否则「读完反馈 / 想一想 / 走开一会儿」的时间
        // 会被算进下一块的学习时长里。
        if run.status != TrainingRunStatus::Active {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockOutOfOrder,
                format!(
                    "训练 {} 的状态是 {}，不是 active —— 不能激活块（FIX E）",
                    run.id,
                    run.status.as_str()
                ),
            ));
        }
        if block.status != TrainingBlockStatus::Pending {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockOutOfOrder,
                format!(
                    "块 {} 的状态是 {}，只有 pending 块可以被激活（FIX E）",
                    block.id,
                    block.status.as_str()
                ),
            ));
        }
        if run.current_block_ordinal != Some(block.ordinal) {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockOutOfOrder,
                format!(
                    "块 {} 的 ordinal 是 {}，但当前应进行的是 {:?} —— 不能越序激活（FIX E）",
                    block.id, block.ordinal, run.current_block_ordinal
                ),
            ));
        }

        // §10 `idx_training_blocks_one_active`：一次至多一个 active 块。
        // 提前给出可读诊断，而不是让唯一索引抛一个难懂的约束错误。
        let active_other: Option<i64> = match tx.query_row(
            "SELECT id FROM training_block_runs
              WHERE training_run_id = ?1 AND status = 'active' AND id <> ?2
              LIMIT 1",
            params![run.id, block_run_id],
            |r| r.get(0),
        ) {
            Ok(v) => Some(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => return Err(TrainingError::db(e)),
        };
        if let Some(other) = active_other {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockOutOfOrder,
                format!(
                    "块 {other} 仍是 active，同一训练一次只能有一个进行中的块（§10 / FIX E）；\
                     请先结束它再开始块 {block_run_id}"
                ),
            ));
        }

        transition_block_status(block.status, TrainingBlockStatus::Active)?;
        tx.execute(
            "UPDATE training_block_runs
                SET status = 'active',
                    started_at = COALESCE(started_at, datetime('now')),
                    updated_at = datetime('now')
              WHERE id = ?1 AND profile_id = ?2",
            params![block_run_id, profile_id],
        )
        .map_err(TrainingError::db)?;
        tx.execute(
            "UPDATE training_runs
                SET current_block_ordinal = ?1,
                    updated_at = datetime('now')
              WHERE id = ?2 AND profile_id = ?3",
            params![block.ordinal, run.id, profile_id],
        )
        .map_err(TrainingError::db)?;

        load_block_run(tx, profile_id, block_run_id)
    })();
    finish_immediate(conn, result)
}

/// 只读求值：这个块当前的完成契约状态（D19：前端不得自己判）。
pub fn block_completion_state(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    elapsed_minutes: Option<i64>,
) -> Result<BlockCompletionState, TrainingError> {
    let block = load_block_run(conn, profile_id, block_run_id)?;
    if block.training_run_id != training_run_id {
        return Err(TrainingError::new(
            TrainingErrorCode::TrainingBlockNotFound,
            format!("块 {block_run_id} 不属于训练 {training_run_id}"),
        ));
    }
    let kind = completion_rule_for(&block)?;
    let facts = gather_completion_facts(conn, profile_id, &block, elapsed_minutes, false, false)?;
    let decision = evaluate_completion(kind, &facts);
    Ok(BlockCompletionState {
        block_run_id: block.id,
        rule_kind: kind,
        rule_zh: match block.protocol_id {
            Some(pid) => find_protocol(pid)
                .completion_rule
                .description_zh
                .to_string(),
            None => "休息片刻".to_string(),
        },
        satisfied: decision.satisfied(),
        reason: decision.reason,
    })
}

/// 纯规则推进：只有冻结完成规则被满足时才落库推进。
///
/// 这是「用户什么都没说，只是完成了一次交互之后」该调的路径。
/// 规则没满足 → **什么都不写**，返回 `advanced = false` 与原因码。
///
/// 本函数**不**产生任何学习证据（D11 / D18 / D21）。
pub fn try_complete_training_block(
    conn: &Connection,
    p: TryCompleteBlockParams,
) -> Result<BlockAdvanceOutcome, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<BlockAdvanceOutcome, TrainingError> {
        let run = load_run(tx, p.profile_id, p.training_run_id)?;
        if run.status.is_terminal() {
            return Err(TrainingError::new(
                TrainingErrorCode::TerminalRunState,
                format!("训练 {} 已处于终态，不再推进块（§9）", run.id),
            ));
        }
        let block = load_block_run(tx, p.profile_id, p.block_run_id)?;
        if block.training_run_id != run.id {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockNotFound,
                format!(
                    "块 {} 不属于训练 {}（跨 run 引用被拒绝）",
                    p.block_run_id, p.training_run_id
                ),
            ));
        }

        let kind = completion_rule_for(&block)?;
        let facts =
            gather_completion_facts(tx, p.profile_id, &block, p.elapsed_minutes, false, false)?;
        let decision = evaluate_completion(kind, &facts);

        let Some(progression) = decision.progression else {
            return Ok(BlockAdvanceOutcome {
                run,
                block,
                progression: None,
                reason: decision.reason,
                advanced: false,
                next_block_id: None,
                learning_moment_ids: Vec::new(),
                fsrs_applied: false,
            });
        };

        let (updated, next_block_id) =
            apply_block_terminal(tx, &run, &block, terminal_status_for(progression))?;
        let run = load_run(tx, p.profile_id, p.training_run_id)?;
        Ok(BlockAdvanceOutcome {
            run,
            block: updated,
            progression: Some(progression),
            reason: decision.reason,
            advanced: true,
            next_block_id,
            // D11 / D21：块推进**永远**不产生学习证据。
            // 这两个字段不是占位符，是可供调用方与测试断言的契约。
            learning_moment_ids: Vec::new(),
            fsrs_applied: false,
        })
    })();
    finish_immediate(conn, result)
}

/// 用户权威推进：用户说了算，**一定**可以往下走。
///
/// 但「可以往下走」不等于「学会了」：
///
/// ```text
/// Finish  用户「做完了 / 继续」  → 块 Completed（D12 `OrExplicit`）
///                                 但不等于 explanation / practice / recall 成功
/// Stop    用户「停下 / 跳过」    → 块 Skipped（D13 user-stop 路径）
///                                 但不等于 error_corrected，也不等于失败（§50）
/// ```
///
/// 只有当冻结规则**已经**被真实交互结果满足时，才记 `Completed` + `RuleSatisfied`；
/// 否则按上面的表记终态，并在两种情况下都**不**写任何学习证据。
///
/// 对 `ErrorCorrection` 尤其重要：用户停止 = 「训练在没有核实到修正的情况下结束」，
/// 绝不写成「已修正」（D13）。
pub fn advance_training_block(
    conn: &Connection,
    p: AdvanceBlockParams,
) -> Result<BlockAdvanceOutcome, TrainingError> {
    begin_immediate(conn)?;
    let tx: &Connection = conn;
    let result = (|| -> Result<BlockAdvanceOutcome, TrainingError> {
        let run = load_run(tx, p.profile_id, p.training_run_id)?;
        if run.status.is_terminal() {
            return Err(TrainingError::new(
                TrainingErrorCode::TerminalRunState,
                format!("训练 {} 已处于终态，不再推进块（§9）", run.id),
            ));
        }
        let block = load_block_run(tx, p.profile_id, p.block_run_id)?;
        if block.training_run_id != run.id {
            return Err(TrainingError::new(
                TrainingErrorCode::TrainingBlockNotFound,
                format!(
                    "块 {} 不属于训练 {}（跨 run 引用被拒绝）",
                    p.block_run_id, p.training_run_id
                ),
            ));
        }

        let kind = completion_rule_for(&block)?;
        let facts = gather_completion_facts(
            tx,
            p.profile_id,
            &block,
            p.elapsed_minutes,
            p.intent == BlockAdvanceIntent::Finish,
            p.intent == BlockAdvanceIntent::Stop,
        )?;
        let mut decision = evaluate_completion(kind, &facts);

        // 用户权威：规则没给出任何推进理由时，由用户的意图兜底。
        // 这一步**在求值器之后**，兜底只决定「凭什么往下走」这个标注，
        // 不改变「有没有产生证据」—— 后者恒为「没有」（D11 / D21）。
        if decision.progression.is_none() {
            decision.progression = Some(match p.intent {
                BlockAdvanceIntent::Finish => BlockProgression::UserFinished,
                BlockAdvanceIntent::Stop => BlockProgression::UserStopped,
            });
            decision.reason = match p.intent {
                BlockAdvanceIntent::Finish => super::completion::REASON_USER_FINISHED.to_string(),
                BlockAdvanceIntent::Stop => super::completion::REASON_USER_STOPPED.to_string(),
            };
        }

        let progression = decision.progression.expect("上面已确保有值");
        let (updated, next_block_id) =
            apply_block_terminal(tx, &run, &block, terminal_status_for(progression))?;
        let run = load_run(tx, p.profile_id, p.training_run_id)?;
        Ok(BlockAdvanceOutcome {
            run,
            block: updated,
            progression: Some(progression),
            reason: decision.reason,
            advanced: true,
            next_block_id,
            learning_moment_ids: Vec::new(),
            fsrs_applied: false,
        })
    })();
    finish_immediate(conn, result)
}

// ============================ 读取 ============================

pub fn get_training_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingRun, TrainingError> {
    load_run(conn, profile_id, training_run_id)
}

pub fn list_block_runs(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<Vec<TrainingBlockRun>, TrainingError> {
    let sql = format!(
        "SELECT {BLOCK_COLUMNS} FROM training_block_runs
          WHERE profile_id = ?1 AND training_run_id = ?2
          ORDER BY ordinal ASC"
    );
    let mut stmt = conn.prepare(&sql).map_err(TrainingError::db)?;
    let rows = stmt
        .query_map(params![profile_id, training_run_id], row_to_block)
        .map_err(TrainingError::db)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(TrainingError::db)
}

/// 列出该档案当前的未终结训练（§8 的唯一开放位，至多一个）。
pub fn find_open_training_run(
    conn: &Connection,
    profile_id: i64,
) -> Result<Option<TrainingRun>, TrainingError> {
    let sql = format!(
        "SELECT {RUN_COLUMNS} FROM training_runs
          WHERE profile_id = ?1 AND status IN ('ready','active','paused')
          ORDER BY id DESC LIMIT 1"
    );
    match conn.query_row(&sql, params![profile_id], row_to_run) {
        Ok(run) => Ok(Some(run)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TrainingError::db(e)),
    }
}

/// W6 §11.1 —— 这条 StudySession 是否由一条**未终结**的训练拥有。
///
/// 反向读取**既有**列 `training_runs.study_session_id`，不新增任何映射表。
/// 「未终结」与 §8 的唯一开放位同口径（`ready` / `active` / `paused`）——
/// `completed` / `abandoned` 的训练**不再**拥有这条会话。
///
/// 返回 `Option<i64>` 而不是 `bool`：调用方真正需要的是「回到哪一条训练」，
/// 而不是一个还要二次查询才能用的布尔值。
pub fn find_open_run_id_for_session(
    conn: &Connection,
    profile_id: i64,
    study_session_id: i64,
) -> Result<Option<i64>, TrainingError> {
    let sql = "SELECT id FROM training_runs
                WHERE profile_id = ?1 AND study_session_id = ?2
                  AND status IN ('ready','active','paused')
                ORDER BY id DESC LIMIT 1";
    match conn.query_row(sql, params![profile_id, study_session_id], |r| r.get(0)) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TrainingError::db(e)),
    }
}

/// 列出该训练的全部交互（按写入顺序），用于前端恢复与审计。
pub fn list_interactions(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<Vec<TrainingInteraction>, TrainingError> {
    let sql = format!(
        "SELECT {INTERACTION_COLUMNS} FROM training_interactions
          WHERE profile_id = ?1 AND training_run_id = ?2
          ORDER BY id ASC"
    );
    let mut stmt = conn.prepare(&sql).map_err(TrainingError::db)?;
    let rows = stmt
        .query_map(params![profile_id, training_run_id], row_to_interaction)
        .map_err(TrainingError::db)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(TrainingError::db)
}

fn load_run(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingRun, TrainingError> {
    let sql = format!("SELECT {RUN_COLUMNS} FROM training_runs WHERE id = ?1 AND profile_id = ?2");
    match conn.query_row(&sql, params![training_run_id, profile_id], row_to_run) {
        Ok(run) => Ok(run),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(TrainingError::new(
            TrainingErrorCode::TrainingRunNotFound,
            format!("训练不存在或不属于该档案（run={training_run_id}, profile={profile_id}）"),
        )),
        Err(e) => Err(TrainingError::db(e)),
    }
}

fn load_block_run(
    conn: &Connection,
    profile_id: i64,
    block_run_id: i64,
) -> Result<TrainingBlockRun, TrainingError> {
    let sql = format!(
        "SELECT {BLOCK_COLUMNS} FROM training_block_runs WHERE id = ?1 AND profile_id = ?2"
    );
    match conn.query_row(&sql, params![block_run_id, profile_id], row_to_block) {
        Ok(block) => Ok(block),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(TrainingError::new(
            TrainingErrorCode::TrainingBlockNotFound,
            format!("训练块不存在或不属于该档案（block={block_run_id}, profile={profile_id}）"),
        )),
        Err(e) => Err(TrainingError::db(e)),
    }
}

fn load_interaction(
    conn: &Connection,
    profile_id: i64,
    interaction_id: i64,
) -> Result<TrainingInteraction, TrainingError> {
    let sql = format!(
        "SELECT {INTERACTION_COLUMNS} FROM training_interactions WHERE id = ?1 AND profile_id = ?2"
    );
    conn.query_row(
        &sql,
        params![interaction_id, profile_id],
        row_to_interaction,
    )
    .map_err(TrainingError::db)
}

fn find_interaction_by_action_id(
    conn: &Connection,
    profile_id: i64,
    client_action_id: &str,
) -> Result<Option<TrainingInteraction>, TrainingError> {
    let sql = format!(
        "SELECT {INTERACTION_COLUMNS} FROM training_interactions
          WHERE profile_id = ?1 AND client_action_id = ?2
          LIMIT 1"
    );
    match conn.query_row(
        &sql,
        params![profile_id, client_action_id],
        row_to_interaction,
    ) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(TrainingError::db(e)),
    }
}

// ============================ 事务辅助 ============================

/// §15 / §19 / §20 统一使用 `BEGIN IMMEDIATE`。
///
/// 仓库未启用 rusqlite 的 `TransactionBehavior`（见 `repository/study_profile.rs` 的说明），
/// 因此沿用既有 `execute_batch` 写法，与仓库其余事务代码保持一致。
///
/// 返回 `()` 而不是某个事务句柄是刻意的：SQLite 的事务属于**连接**，不属于句柄。
/// 开启之后，调用方继续使用同一个 `&Connection` 完成全部读写即可。
fn begin_immediate(conn: &Connection) -> Result<(), TrainingError> {
    conn.execute_batch("BEGIN IMMEDIATE").map_err(|e| {
        TrainingError::new(
            TrainingErrorCode::Db,
            format!("无法开始 IMMEDIATE 事务：{e}"),
        )
    })
}

/// 成功 → `COMMIT`；失败 → `ROLLBACK` 后原样返回错误。
fn finish_immediate<T>(
    conn: &Connection,
    result: Result<T, TrainingError>,
) -> Result<T, TrainingError> {
    match result {
        Ok(value) => {
            conn.execute_batch("COMMIT").map_err(TrainingError::db)?;
            Ok(value)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

// ============================ GROUNDED LEARNING BRIDGE V1 · P1.1 事务边界证明 ============================

/// 这些测试必须在**事务边界**上取证，而不是走整条 Today 管线：
///
/// ```text
/// OM-P1-17  准备好的材料与计划不一致 → 在**提交真相之前**被拒绝，且不留痕
/// OM-P1-07  休息块的快照保持 NULL；学习块拿到材料
/// OM-P1-09  快照落库失败 → **整个**创建事务回滚
/// OM-P1-16  同上：不允许「Run 已提交、快照却写失败」这种半成品
/// ```
///
/// `create_training_run_with_materials` 是 `pub(crate)`，因此这里用同模块单元测试；
/// 生产路径（`start_training_for_item`）的同类证明放在
/// `tests/grounded_learning_bridge_closure.rs`，两处都不依赖另一处。
#[cfg(test)]
mod p1_transaction_boundary_tests {
    use super::*;
    use crate::cognitive::protocol::{find, CompletionRule, CompletionRuleKind};
    use crate::cognitive::session_composer::{TrainingBlock, TrainingSessionPlan};
    use crate::repository::learning_item::LearningItemRepository;
    use crate::repository::study_profile::StudyProfileRepository;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn mk_profile(conn: &Connection, name: &str) -> i64 {
        StudyProfileRepository::new(conn)
            .create(name, None, None, None, None, None)
            .unwrap()
            .id
    }

    fn mk_item(conn: &Connection, profile_id: i64, name: &str) -> i64 {
        LearningItemRepository::new(conn)
            .create_for_profile(profile_id, None, name, None, None)
            .unwrap()
            .id
    }

    /// 一块「学习块 + 休息块」的确定性计划（休息块**必须**存在，才能证明 NULL 语义）。
    fn plan_learning_then_break(target: Option<i64>) -> TrainingSessionPlan {
        TrainingSessionPlan {
            target_learning_item_id: target,
            total_minutes: 15,
            blocks: vec![
                TrainingBlock {
                    ordinal: 0,
                    protocol_id: Some(ProtocolId::FreeRecall),
                    minutes: 10,
                    goal: find(ProtocolId::FreeRecall).goal.to_string(),
                    completion_rule: find(ProtocolId::FreeRecall).completion_rule,
                    is_break: false,
                },
                TrainingBlock {
                    ordinal: 1,
                    protocol_id: None,
                    minutes: 5,
                    goal: "休息".to_string(),
                    completion_rule: CompletionRule {
                        kind: CompletionRuleKind::TimeSliceOrUserStop,
                        description_zh: "休息不计入学习证据",
                    },
                    is_break: true,
                },
            ],
            reason_codes: Vec::new(),
            evidence_refs: Vec::new(),
        }
    }

    fn material_for(protocol: ProtocolId) -> GroundedTrainingMaterial {
        GroundedTrainingMaterial {
            version: 1,
            status: crate::training::grounded_material::MaterialStatus::Ready,
            protocol_id: protocol.as_str().to_string(),
            prompt_text: None,
            cue_text: Some("cue".to_string()),
            source_excerpt: Some("excerpt".to_string()),
            reference_text: None,
            worked_steps: Vec::new(),
            hidden_step_index: None,
            practice_prompt: None,
            transfer_prompt: None,
            generated_by: crate::training::grounded_material::GeneratedBy::Deterministic,
            provenance: Vec::new(),
            unavailable_reason: None,
        }
    }

    fn prepared(ordinal: i64, protocol: ProtocolId) -> PreparedBlockMaterial {
        PreparedBlockMaterial {
            ordinal,
            protocol_id: protocol,
            material: material_for(protocol),
        }
    }

    fn params(profile_id: i64, plan: TrainingSessionPlan) -> CreateTrainingRunParams {
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: plan.target_learning_item_id,
            mode: DecisionMode::default(),
            plan,
            now_utc: "2026-09-19 00:00:00".to_string(),
        }
    }

    fn counts(conn: &Connection) -> (i64, i64, i64) {
        let runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM training_runs", [], |r| r.get(0))
            .unwrap();
        let blocks: i64 = conn
            .query_row("SELECT COUNT(*) FROM training_block_runs", [], |r| r.get(0))
            .unwrap();
        let sessions: i64 = conn
            .query_row("SELECT COUNT(*) FROM study_sessions", [], |r| r.get(0))
            .unwrap();
        (runs, blocks, sessions)
    }

    fn snapshot_of(conn: &Connection, ordinal: i64) -> Option<String> {
        conn.query_row(
            "SELECT material_snapshot_json FROM training_block_runs WHERE ordinal = ?1",
            params![ordinal],
            |r| r.get(0),
        )
        .unwrap()
    }

    // ---- OM-P1-17：材料与计划不一致 → 提交真相之前拒绝 ----

    #[test]
    fn p1_17_mismatched_prepared_materials_are_refused_before_any_truth_is_committed() {
        let conn = setup();
        let p = mk_profile(&conn, "P1.17");

        // (标签, prepared 集合)
        let cases: Vec<(&str, Vec<PreparedBlockMaterial>)> = vec![
            (
                "ordinal 不在计划里",
                vec![prepared(99, ProtocolId::FreeRecall)],
            ),
            (
                "ordinal 指向休息块",
                vec![prepared(1, ProtocolId::FreeRecall)],
            ),
            (
                "与块的协议不一致",
                vec![prepared(0, ProtocolId::CuedRecall)],
            ),
            (
                "ordinal 重复",
                vec![
                    prepared(0, ProtocolId::FreeRecall),
                    prepared(0, ProtocolId::FreeRecall),
                ],
            ),
        ];

        for (label, prepared_materials) in cases {
            let before = counts(&conn);
            let err = create_training_run_with_materials(
                &conn,
                params(p, plan_learning_then_break(None)),
                &prepared_materials,
            )
            .expect_err(&format!("{label} 必须被拒绝"));

            assert_eq!(
                err.code,
                TrainingErrorCode::PreparedMaterialMismatch,
                "{label}：必须是 typed error（{err}）"
            );
            assert_eq!(counts(&conn), before, "{label}：拒绝不得留下任何真相");
            assert_eq!(
                counts(&conn),
                (0, 0, 0),
                "{label}：事务前拒绝，连 Session 都不该建"
            );
        }

        // 材料内部的 protocol_id 与 join key 不一致 → 同样拒绝。
        let mut bad = prepared(0, ProtocolId::FreeRecall);
        bad.material.protocol_id = ProtocolId::CuedRecall.as_str().to_string();
        let err = create_training_run_with_materials(
            &conn,
            params(p, plan_learning_then_break(None)),
            &[bad],
        )
        .expect_err("材料内部协议与 ordinal 协议不一致必须被拒绝");
        assert_eq!(err.code, TrainingErrorCode::PreparedMaterialMismatch);
        assert_eq!(counts(&conn), (0, 0, 0));
    }

    // ---- OM-P1-07 + OM-P1-01：学习块有快照，休息块保持 NULL ----

    #[test]
    fn p1_07_learning_block_gets_snapshot_and_break_block_stays_null() {
        let conn = setup();
        let p = mk_profile(&conn, "P1.07");
        let item = mk_item(&conn, p, "线粒体");

        let (run, blocks) = create_training_run_with_materials(
            &conn,
            params(p, plan_learning_then_break(Some(item))),
            &[prepared(0, ProtocolId::FreeRecall)],
        )
        .unwrap();

        assert_eq!(blocks.len(), 2);
        let learning = blocks.iter().find(|b| b.ordinal == 0).unwrap();
        let rest = blocks.iter().find(|b| b.ordinal == 1).unwrap();
        assert!(!learning.is_break);
        assert!(rest.is_break);

        let json = snapshot_of(&conn, 0).expect("学习块必须有接地快照");
        let parsed: GroundedTrainingMaterial = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol_id, ProtocolId::FreeRecall.as_str());
        assert_eq!(
            parsed.status,
            crate::training::grounded_material::MaterialStatus::Ready
        );

        assert!(
            snapshot_of(&conn, 1).is_none(),
            "休息块的 material_snapshot_json 必须保持 NULL（P1.3）"
        );

        // 读侧（既有唯一读取入口）必须能读回同一份东西。
        let read_back = crate::training::load_material_snapshot(&conn, p, learning.id).unwrap();
        assert_eq!(
            read_back.map(|m| m.protocol_id),
            Some("free_recall".to_string())
        );

        // 别的档案读不到（既有 profile 隔离，不是新规则）。
        let other = mk_profile(&conn, "别的档案");
        assert!(
            crate::training::load_material_snapshot(&conn, other, learning.id)
                .unwrap()
                .is_none(),
            "跨档案读取必须拿不到任何快照"
        );
        // 没被请求的块同样保持 NULL（这里 ordinal 1 已证明；再确认 run 落库）。
        let persisted_blocks: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM training_block_runs WHERE training_run_id = ?1",
                params![run.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(persisted_blocks, 2);
    }

    // ---- OM-P1-09 / OM-P1-16：快照写失败 → 整个创建事务回滚 ----

    #[test]
    fn p1_16_snapshot_write_failure_rolls_back_the_whole_create_transaction() {
        let conn = setup();
        let p = mk_profile(&conn, "P1.16");

        // 真实 DB 层故障注入：任何一次「把快照写成非 NULL」的 UPDATE 都被 ABORT。
        // 这不是 mock 事务，而是让**真实的**表约束真的失败。
        conn.execute_batch(
            "CREATE TRIGGER p1_fault_injection
             BEFORE UPDATE OF material_snapshot_json ON training_block_runs
             WHEN NEW.material_snapshot_json IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'p1.16 fault injection'); END;",
        )
        .unwrap();

        let err = create_training_run_with_materials(
            &conn,
            params(p, plan_learning_then_break(None)),
            &[prepared(0, ProtocolId::FreeRecall)],
        )
        .expect_err("快照写失败必须让整次创建失败");

        assert_eq!(
            err.code,
            TrainingErrorCode::GroundedSnapshotPersistFailed,
            "必须是可识别的快照落库失败（{err}）"
        );
        assert_eq!(
            counts(&conn),
            (0, 0, 0),
            "OM-P1-16：不允许留下「Run 已提交但无接地」的半成品 —— Run / 块 / Session 全部回滚"
        );

        // 移除故障后，同样的调用必须成功 —— 证明失败确实来自被注入的那一步，
        // 而不是别的原因（「失败理由必须与假设一致」）。
        conn.execute_batch("DROP TRIGGER p1_fault_injection")
            .unwrap();
        let (run, blocks) = create_training_run_with_materials(
            &conn,
            params(p, plan_learning_then_break(None)),
            &[prepared(0, ProtocolId::FreeRecall)],
        )
        .unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(snapshot_of(&conn, 0).is_some());
        assert_eq!(counts(&conn), (1, 2, 1));
        let _ = run;
    }
}
