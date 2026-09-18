//! REAL LEARNING ENGINE V1 · W4：TrainingExperience 的 IPC 入口（§19 / §15 / §20）。
//!
//! # 为什么命令层这么薄
//!
//! 与 `commands/mod.rs` 的既有约定一致：命令层只做**参数解析 + DB 锁获取**，
//! 全部业务逻辑在 `crate::training`。这里刻意**没有**任何状态机、没有幂等判断、
//! 没有 FSRS 调用 —— 那些一旦搬到命令层，就会变成第二个真相源。
//!
//! # 计划从哪来（§19）
//!
//! `create_training_run_for_item` **不接受前端传来的计划**。
//!
//! 理由：`TrainingSessionPlan` 是 Cognitive Core V1.2 的**确定性产出**
//! （`session_composer::compose_session`）。如果让前端把计划对象回传，
//! 就等于允许前端编排教学法 —— 那正是任务书要移除的「executor architecture
//! authority」。所以这里用与 Today Coach **完全相同**的确定性路径重新编排一次，
//! 前端只说「我打算学多久」。
//!
//! # 幂等键属于谁（§13）
//!
//! `client_action_id` 由**前端**为每一次用户动作生成一次，并在网络重试时复用。
//! 后端不做任何补全或去重猜测：键是什么，就用什么。

use crate::db;
use crate::training;
use crate::training::grounded_material::GroundedTrainingMaterial;
use crate::training::types::{BlockAdvanceIntent, InteractionResult, VerificationMethod};
use rusqlite::OptionalExtension;

/// §19 创建训练的结果：新 run + 被物化出来的块。
///
/// 前端拿到 `blocks` 后**不得**重排、增删或重算时长 —— 顺序与时长都是
/// 后端已经落库的事实。
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
pub struct StartTrainingResponse {
    pub run: training::TrainingRun,
    pub blocks: Vec<training::TrainingBlockRun>,
}

/// §19：开始一次训练。
///
/// 硬约束：
/// - **原子**：run + 全部块 + StudySession 绑定 + DIRECT intent 消费在同一事务内；
/// - **不接受前端计划**（见模块头说明），也**不接受前端指定的学习项** ——
///   学什么是计划决定的（`plan.target_learning_item_id`）；
/// - `available_minutes = None` → `NO_AVAILABLE_MINUTES`，不编造默认时长；
/// - 一个档案同时只能有一个未终结的 run（`OpenTrainingRunExists`）。
#[tauri::command]
pub fn create_training_run_for_item(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    available_minutes: Option<i64>,
) -> Result<StartTrainingResponse, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::start_training_for_item(&conn, profile_id, available_minutes)
        .map(|(run, blocks)| StartTrainingResponse { run, blocks })
        .map_err(|e| e.to_string())
}

/// 读取一次训练的全部持久化状态（run + 块 + 已发生的交互）。
///
/// **只读**，且一次 IPC 返回整页所需 —— 前端不得为每个块各开一个命令。
#[tauri::command]
pub fn get_training_session(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingSessionView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::load_training_session(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}

/// §13 / §15：记录一次用户交互。
///
/// `client_action_id` 由前端生成；网络重试必须复用同一个值 —— 命中后
/// 原样返回既有结果（`replayed = true`），不产生第二个学习事实。
/// 同一个键配上不同 payload → `IDEMPOTENCY_KEY_REUSED_WITH_DIFFERENT_PAYLOAD`。
///
/// # HOTFIX-01 FIX A1 —— 前端**不能**选择判定方式
///
/// 这个命令**没有** `verification` 参数，这是刻意的，而且它就是 FIX A1 的全部内容：
///
/// ```text
/// 前端不能把 SelfCheck 提升成 Deterministic / Structured
/// 前端也不能自己指定 AiTutor
/// ```
///
/// 手工前端提交在后端一律记 `SelfCheck`（非权威，证据质量上限 MEDIUM）。
/// `Deterministic` / `Structured` 只允许由**真实执行过的**后端验证器签发
/// （FIX A4：PACK A 不发明验证器），因此它们不可能经由 IPC 从外部获得。
///
/// 删掉参数而不是「忽略前端传来的值」：一个不存在的参数是**结构性**保证，
/// 而「接收后覆盖」只是约定 —— 后者会在下一次有人重构时悄悄失效。
///
/// `moment_type` 同样不再由调用方声明（FIX B）：它由 runtime 从
/// `(ProtocolId, interaction_type, result, verification)` 推导。
#[tauri::command]
pub fn record_training_interaction(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    client_action_id: String,
    interaction_type: String,
    user_response_text: Option<String>,
    prompt_text: Option<String>,
    hint_level: Option<i64>,
    result: Option<InteractionResult>,
    occurred_at: Option<String>,
) -> Result<training::InteractionOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;

    // FIX A1：手工提交的唯一判定方式。前端无从选择，因此这里不是「默认值」，
    // 而是**该通路的定义**。
    let verification = VerificationMethod::SelfCheck;

    crate::training::record_interaction(
        &conn,
        training::RecordInteractionParams {
            profile_id,
            training_run_id,
            block_run_id,
            client_action_id,
            interaction_type,
            prompt_text,
            user_response_text,
            hint_level,
            result,
            verification,
            // 原样透传：`None` 表示「用领域层的当前时间」。
            // 命令层刻意不在这里取时钟 —— 见 `RecordInteractionParams::occurred_at`
            // 的注释：格式若有第二个来源，就会静默污染按时间排序的真相。
            occurred_at,
        },
    )
    .map_err(|e| e.to_string())
}

/// HOTFIX-01 FIX D：**唯一**的初始启动通路。
///
/// 前端「开始这次训练」按钮必须调用它，而**不是**
/// `transition_training_run(..., Active)` —— 后者只改 run 状态，
/// 会留下一个没有任何活跃块的 active 训练（见 `training::start_training_run`）。
#[tauri::command]
pub fn start_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::start_training_run(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}

/// HOTFIX-01 FIX F2：提前结束训练（剩余块 → Skipped，零成功证据，零 FSRS）。
#[tauri::command]
pub fn abandon_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::abandon_training_run(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}

/// §9：推进 run 状态（`ready → active → paused → completed / abandoned`）。
///
/// 非法迁移返回 typed error，而不是静默改成合法值。
#[tauri::command]
pub fn transition_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    to: training::TrainingRunStatus,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::transition_training_run(&conn, profile_id, training_run_id, to)
        .map_err(|e| e.to_string())
}

/// 激活一个块（写入 `started_at`，成为当前块）。
///
/// 存在的理由：时间片完成规则需要块计时状态（D17），
/// 而没有被激活过的块没有 `started_at` —— 时间片将永远无法走完。
#[tauri::command]
pub fn start_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
) -> Result<training::TrainingBlockRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::start_training_block(&conn, profile_id, training_run_id, block_run_id)
        .map_err(|e| e.to_string())
}

/// 用户权威推进一个块（D12 `finish` / D13 `stop`）。
///
/// **前端永远不得自己判定「这个块做完了」**（D19）。它只能表达
/// 「用户按了哪个键」，由后端走冻结完成规则求值后决定终态。
///
/// 无论结果如何，这条命令都**不会**产生任何学习证据（D11 / D21）：
/// 返回的 `learning_moment_ids` 恒为空、`fsrs_applied` 恒为 false。
#[tauri::command]
pub fn advance_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    intent: BlockAdvanceIntent,
    elapsed_minutes: Option<i64>,
) -> Result<training::BlockAdvanceOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::advance_training_block(
        &conn,
        crate::training::AdvanceBlockParams {
            profile_id,
            training_run_id,
            block_run_id,
            intent,
            elapsed_minutes,
        },
    )
    .map_err(|e| e.to_string())
}

/// 纯规则推进：只有冻结完成规则**已经**被满足时才往前走。
///
/// 规则未满足 → `advanced = false`，并且**什么都不写**。
/// 这是「用户刚提交了一次交互」之后该调的路径。
#[tauri::command]
pub fn try_complete_training_block(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
    block_run_id: i64,
    elapsed_minutes: Option<i64>,
) -> Result<training::BlockAdvanceOutcome, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::try_complete_training_block(
        &conn,
        crate::training::TryCompleteBlockParams {
            profile_id,
            training_run_id,
            block_run_id,
            elapsed_minutes,
        },
    )
    .map_err(|e| e.to_string())
}

/// §20：原子完成一次训练（结束 StudySession 与 run 在同一个事务内）。
#[tauri::command]
pub fn complete_training_run(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    training_run_id: i64,
) -> Result<training::TrainingRun, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    crate::training::complete_training_run(&conn, profile_id, training_run_id)
        .map_err(|e| e.to_string())
}

// ============================ GROUNDED LEARNING BRIDGE V1 · W5 ============================

/// §10.9 —— 一条**人类可读**的出处标签。
///
/// UI 只渲染 `display_name`（+ 有则 `section_title`），**绝不**把 `source_id` /
/// `section_id` 这类内部行号展示给普通用户。id 保留在这里只是为了让前端能把
/// 标签与快照里的 provenance 对上，不是给人看的。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
pub struct GroundedProvenanceLabel {
    pub source_id: i64,
    pub display_name: String,
    pub section_id: Option<i64>,
    pub section_title: Option<String>,
}

/// 某个训练块的接地材料视图：快照本身 + 已解析好的出处标签。
///
/// `material = None` 表示这个块**没有**快照（旧块 / 尚未接地）—— 那是「没有」，
/// 不是「加载失败」。八个专项体验据此显示各自诚实的不可用状态。
#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct GroundedMaterialView {
    pub material: Option<GroundedTrainingMaterial>,
    pub provenance_labels: Vec<GroundedProvenanceLabel>,
}

/// 读取某个训练块落库的材料快照，并把出处解析成可读标签。
///
/// # 为什么放在 `commands/training.rs`
///
/// 它是训练块的**读取**入口，和 `get_training_session` 同层：命令层只做
/// 「加锁 + 调下层」，没有任何编排逻辑。
///
/// # 边界
///
/// - **profile 隔离**：快照与标签都先按 `profile_id` 过滤，跨档案一律拿不到。
/// - **只读**：不产生 `LearningMoment` / `Evidence` / `MemoryReview`，不推进 FSRS。
///   `example_view` 之类的「看一眼」动作永远不等于掌握度。
/// - 快照不存在 → `material = None`（**不是**错误，也不编造一份材料）。
///
/// 命令与测试共用同一个 core 实现，避免出现第二套投影逻辑。
pub fn block_grounded_material_core(
    conn: &rusqlite::Connection,
    profile_id: i64,
    block_run_id: i64,
) -> Result<GroundedMaterialView, String> {
    let material = crate::training::load_material_snapshot(conn, profile_id, block_run_id)?;
    let Some(material) = material else {
        return Ok(GroundedMaterialView {
            material: None,
            provenance_labels: Vec::new(),
        });
    };

    let mut provenance_labels: Vec<GroundedProvenanceLabel> = Vec::new();
    for r in &material.provenance {
        // 来源必须在同一档案内；查不到就跳过这一条（不编造标签）。
        let display_name: Option<String> = conn
            .query_row(
                "SELECT display_name FROM document_sources WHERE id = ?1 AND profile_id = ?2",
                rusqlite::params![r.source_id, profile_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(display_name) = display_name else {
            continue;
        };

        let section_title: Option<String> = match r.section_id {
            Some(section_id) => conn
                .query_row(
                    "SELECT title FROM document_sections WHERE id = ?1 AND profile_id = ?2",
                    rusqlite::params![section_id, profile_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .flatten(),
            None => None,
        };

        let label = GroundedProvenanceLabel {
            source_id: r.source_id,
            display_name,
            section_id: r.section_id,
            section_title,
        };
        if !provenance_labels.contains(&label) {
            provenance_labels.push(label);
        }
    }

    Ok(GroundedMaterialView {
        material: Some(material),
        provenance_labels,
    })
}

/// 读取块材料快照的 IPC 入口（§10.4 / §10.9）。
#[tauri::command]
pub fn get_block_grounded_material(
    state: tauri::State<'_, db::DbState>,
    profile_id: i64,
    block_run_id: i64,
) -> Result<GroundedMaterialView, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    block_grounded_material_core(&conn, profile_id, block_run_id)
}
