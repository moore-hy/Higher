//! REAL LEARNING ENGINE V1 · W4：从「今天该做什么」到「一次可执行的训练」。
//!
//! # 为什么需要这一层
//!
//! `runtime.rs` 只认一份已经编排好的 `TrainingSessionPlan`。但**计划从哪来**
//! 是一个独立且必须唯一回答的问题：如果让前端把计划对象回传，就等于允许前端
//! 编排教学法 —— 那正是任务书要移除的「executor architecture authority」。
//!
//! 所以这里把「编排」与「持久化」串起来，且**只允许一条通路**：
//!
//! ```text
//! 用户选择可用时长
//!   → build_today_coach_snapshot（与 Today Coach **完全相同**的确定性路径）
//!   → snapshot.plan
//!   → create_training_run（§19 原子持久化）
//! ```
//!
//! 前端只说「我打算学多久」，永远不参与协议选择、块编排或时长分配。
//!
//! # 模式从哪来
//!
//! `snapshot.mode` 是**真正生效**的模式（有效意图存在时来自意图，见
//! `today_projection`）。这一点很关键：§5 要求 DIRECT 意图在创建 TrainingRun 时
//! 被同事务消费，而消费条件是 `mode == Direct`。模式由意图决定 → 消费条件成立，
//! 整条链路才闭合。

use rusqlite::Connection;

use crate::cognitive::decision::DecisionMode;
use crate::cognitive::session_composer::TrainingSessionPlan;

use super::grounding::{compile_grounded_material, GroundingRequest};
use super::runtime::{
    block_completion_state, create_training_run_with_materials, get_training_run, list_block_runs,
    list_interactions, BlockCompletionState, CreateTrainingRunParams, PreparedBlockMaterial,
};
use super::types::{
    TrainingBlockRun, TrainingError, TrainingErrorCode, TrainingInteraction, TrainingRun,
};

/// 一次训练的**完整**持久化视图。
///
/// 一次 IPC 返回整页所需（run + 块 + 已发生的交互 + 每个块的完成契约状态）：
/// 前端不得为每个块各开一个命令，否则「后端唯一真相」会被拆成一堆局部读取。
///
/// `completions` 是 D19 的落点：完成契约与「现在能不能往下走」由后端算好一起返回，
/// 前端只负责把**已经写死的冻结规则**讲给用户听，不自己判定。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct TrainingSessionView {
    pub run: TrainingRun,
    pub blocks: Vec<TrainingBlockRun>,
    pub interactions: Vec<TrainingInteraction>,
    pub completions: Vec<BlockCompletionState>,
}

/// 只读读取一次训练的全部状态。
///
/// `completions` 在**未知已用时长**（`elapsed_minutes = None`）下计算：
/// 只读视图拿不到可靠的块计时，而 D17 禁止拿挂钟冒充学习事实。
/// 时间片规则因此在只读视图里会显示「还没走完」—— 这是诚实的，
/// 真正的时间推进发生在 `advance_training_block` / `try_complete_training_block`
/// 由调用方显式提供已用分钟数时（D18）。
pub fn load_training_session(
    conn: &Connection,
    profile_id: i64,
    training_run_id: i64,
) -> Result<TrainingSessionView, TrainingError> {
    let run = get_training_run(conn, profile_id, training_run_id)?;
    let blocks = list_block_runs(conn, profile_id, training_run_id)?;
    let interactions = list_interactions(conn, profile_id, training_run_id)?;
    let mut completions: Vec<BlockCompletionState> = Vec::with_capacity(blocks.len());
    for block in &blocks {
        completions.push(block_completion_state(
            conn,
            profile_id,
            training_run_id,
            block.id,
            None,
        )?);
    }
    Ok(TrainingSessionView {
        run,
        blocks,
        interactions,
        completions,
    })
}

/// GROUNDED LEARNING BRIDGE V1 · P1.1 **PHASE A** —— 在写事务**之外**准备接地材料。
///
/// # 为什么必须在事务之外
///
/// 编译材料会走真实文档检索（FTS / 邻接 / 父上下文）。把它放进创建事务，
/// 只会把写锁持有时间拉长到「一次检索的长度」，而收益为零：材料是**只读**产物，
/// 不依赖任何即将被写入的行。真正必须原子的是**落库**那一步（PHASE B，
/// 在 [`create_training_run_with_materials`] 内）。
///
/// # 锁定语义
///
/// - 材料绑定的是**计划决定的目标学习项**（`plan.target_learning_item_id`），
///   而不是调用方口述的学习项 —— 「run 指向 A、材料来自 B」是不允许的；
/// - 协议取**计划里那一块的协议**，本函数绝不替换协议，也绝不「就近挑一个能用的」；
/// - `ai = None`（§P1.2）：确定性学习闭环必须在**没有云端、没有本地模型、没有
///   provider** 的前提下完整成立。更丰富材料属于后续能力，不在今晚的闭包范围内；
/// - 没有 Ready 来源 → 材料是**诚实的 `Unavailable`**，照常准备并落库（P1.3）：
///   「不可用」不是失败，也不得用编造内容掩盖；
/// - 没有目标学习项 → 不准备任何材料，快照保持 NULL；
/// - 休息块 → 不准备材料，快照保持 NULL。
pub(crate) fn prepare_block_materials(
    conn: &Connection,
    profile_id: i64,
    plan: &TrainingSessionPlan,
    mode: DecisionMode,
) -> Result<Vec<PreparedBlockMaterial>, TrainingError> {
    let Some(item_id) = plan.target_learning_item_id else {
        // 没有目标学习项就没有「该学什么」，因此没有可接地的对象。
        // 这里**不**退化成「全档案语料库」—— 那正是 §9.1 禁止的静默全库检索。
        return Ok(Vec::new());
    };

    let mut prepared: Vec<PreparedBlockMaterial> = Vec::new();
    for block in &plan.blocks {
        if block.is_break {
            continue;
        }
        let Some(protocol) = block.protocol_id else {
            // 非休息块缺协议本身就会被 `validate_block_invariant` 在创建时拒绝。
            // 本函数不替它编一个协议，也不假装它有材料。
            continue;
        };

        let req = GroundingRequest {
            profile_id,
            learning_item_id: item_id,
            protocol,
            block_goal: block.goal.as_str(),
            mode,
        };
        let material = compile_grounded_material(conn, &req, None).map_err(TrainingError::db)?;

        prepared.push(PreparedBlockMaterial {
            ordinal: block.ordinal,
            protocol_id: protocol,
            material,
        });
    }
    Ok(prepared)
}

/// §19：用**与 Today Coach 完全相同的确定性路径**编排并创建一次训练。
///
/// 硬约束：
/// - `available_minutes` 必须是一个真实的正数。缺失 → `NO_AVAILABLE_MINUTES`，
///   **不编造默认时长**（§36：没有数据就说没有数据）；
/// - 计划不可执行（例如候选池为空、Direct 目标不在候选集里）→ `PLAN_HAS_NO_BLOCKS`，
///   诚实空状态而不是随便塞一个块；
/// - `learning_item_id` 取自 `plan.target_learning_item_id` —— **计划决定学什么**，
///   而不是调用方决定，避免「run 指向 A、计划编排的是 B」这种自相矛盾；
/// - 模式取自 `snapshot.mode`，因此 DIRECT 意图会被 `create_training_run`
///   在同一事务内消费（§5）；
/// - **接地材料先编译、后与块同事务落库**（P1.1）。生产路径**永远**不会
///   静默跳过准备：没有 Ready 来源时落库的是一份诚实的 `Unavailable` 快照。
pub fn start_training_for_item(
    conn: &Connection,
    profile_id: i64,
    available_minutes: Option<i64>,
) -> Result<(TrainingRun, Vec<TrainingBlockRun>), TrainingError> {
    let minutes = match available_minutes {
        Some(m) if m > 0 => m,
        _ => {
            return Err(TrainingError::new(
                TrainingErrorCode::NoAvailableMinutes,
                "还没有选择本次可用的时长，无法编排训练（不编造默认时长）".to_string(),
            ))
        }
    };

    let now_utc = crate::cognitive::today_projection::utc_now();

    // 与 Today Coach 同一条确定性路径。`DecisionMode::default()` 只是**兜底**：
    // 若存在有效意图，快照会返回意图里的模式并覆盖它。
    let snapshot = crate::cognitive::build_today_coach_snapshot(
        conn,
        profile_id,
        Some(minutes),
        DecisionMode::default(),
    )
    .map_err(TrainingError::db)?;

    let plan = match snapshot.plan {
        Some(plan) => plan,
        None => {
            return Err(TrainingError::new(
                TrainingErrorCode::PlanHasNoBlocks,
                format!(
                    "当前状态下编排不出可执行的训练计划（{minutes} 分钟）；\
                     这不是失败，而是「现在没有合适的下一步」"
                ),
            ))
        }
    };

    // PHASE A —— 事务外准备。
    let prepared: Vec<PreparedBlockMaterial> =
        prepare_block_materials(conn, profile_id, &plan, snapshot.mode)?;

    // PHASE B —— 同事务落库（run + 全部块 + 接地快照 + DIRECT 意图消费）。
    // 生产路径要求完整覆盖：每个 non-break 学习块恰好 1 份材料（AUDIT REOPEN 项 3）。
    create_training_run_with_materials(
        conn,
        CreateTrainingRunParams {
            profile_id,
            learning_item_id: plan.target_learning_item_id,
            mode: snapshot.mode,
            plan,
            now_utc,
        },
        &prepared,
        true,
    )
}
