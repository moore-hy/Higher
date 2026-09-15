//! v032 · Micro Learning Events（HIGHER DAILY EXPERIENCE V1 · PHASE 3 / PHASE 4）。
//!
//! ## 为什么必须是新表（而不是复用 `evaluations`）
//!
//! 施工前按任务书要求**先定向检查现有 Evaluation / Evidence 能否承载 Micro Evidence**。
//! 结论：**语义不足**，三条硬理由（逐条可验证）：
//!
//! 1. **`duration_seconds` 无处可放**。全仓 `duration_seconds` 仅存在于 `study_sessions`
//!    （v002 建表、v012/v013 重建）。`evaluations` 建表于 v004，v021 只追加了
//!    `session_id / source_kind / source_ref / trust_state`。而任务书 §4.1 要求 Micro
//!    Evidence 至少保存 `duration_seconds`，§4.5 又要求 **Micro duration 独立保存**
//!    且 **Micro 永远不写 StudySession** —— 两者叠加后 `evaluations` 无法表达。
//! 2. **`action_type` 四值不可表达**。`evaluations.evaluation_type` 是 canonical 六值集合
//!    （practice / test / recall / application / project / other，见
//!    `repository::evaluation::EVALUATION_TYPES`）。任务书 PHASE 3 的四种 Micro 动作
//!    （recall / self_explain / retry_recent_error / review_recent_concept）只有 `recall`
//!    能映射，其余三个若强塞 `other` 会破坏该字段的语义，也让 Micro 去重
//!    （DE009：不得机械重复同一种 Micro）无法按动作类型判断。
//! 3. **会污染 canonical Planning Review 证据**。`evaluations` 是
//!    `ai::learning_load::build_learning_load_evidence` 的输入之一，直接进入
//!    `LearningEvidenceSummary.evaluation_count`、`LearningUnitEvidence.evaluation.*`
//!    与 `evidence_quality`。若把 30 秒 Micro 写进 `evaluations`，一次「不看笔记复述一句话」
//!    会改变正式 AI Review 所依赖的 canonical 证据 —— 与 PHASE 0.2 刚刚封存的
//!    fail-closed 安全边界正相反（Evidence 缺失 ≠ 用户没有学习，同理：
//!    30 秒 Micro ≠ 一次正式验证）。
//!
//! 另外 §3.1 的 Micro 来源包含 Task / Session / LearningItem / Goal / Evaluation 五类，
//! `evaluations` 只有单一 `learning_item_id` 外键，无法表达多态来源。
//!
//! 因此本迁移新增 `micro_learning_events`（任务书 §4.4 明确允许「第一版可以有
//! Micro Event Store」，前提是 LearningState 负责统一投影 —— 见
//! `learning_state::micro` 与 `LearningStateSnapshot.recent_micro_actions`）。
//!
//! ## 设计约束
//!
//! - **不是第二套 Evidence 世界**：本表只存 Micro 事实，`learning_state::micro` 把它
//!   投影进同一份 `LearningStateSnapshot`，与 `learning_evidence` 并列消费；
//! - **不写 StudySession**：本表与 `study_sessions` 之间**没有任何外键**，
//!   `session_id` 只作为**来源引用**（source_type='session'），不是归属关系；
//! - **source_id 是弱引用（无 FK）**：source_type 是多态的（task / learning_item /
//!   session / goal / evaluation），SQLite 无法表达多态外键。跨档案归属校验由
//!   `MicroLearningEventRepository::create` 在写入时强制（DE021 覆盖）；
//! - **删除知识节点不级联删除历史 Micro 证据**：证据是历史事实。projection 在
//!   解析不到来源时**静默跳过该候选**（fail-safe，不伪造来源）；
//! - `profile_id` 外键级联：删除学习档案 → 该档案全部 Micro 证据一并清除。
//!
//! Ledger：本次为任务书 §5.1 允许的**下一个连续版本**（施工前最高为 v031）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS micro_learning_events (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id       INTEGER NOT NULL REFERENCES study_profiles(id) ON DELETE CASCADE,
            -- §3.1 允许的 Micro 来源（多态弱引用，无 FK）：evaluation | learning_item |
            -- task | session | goal | none
            source_type      TEXT NOT NULL,
            source_id        INTEGER,
            -- PHASE 3 的四种 Micro Action：recall | self_explain |
            -- retry_recent_error | review_recent_concept
            action_type      TEXT NOT NULL,
            -- §4.1 result：done | partial | skipped
            result           TEXT NOT NULL DEFAULT 'done',
            -- 0-LLM 模板变体 key（有限枚举，不是长文本）
            prompt_variant   TEXT,
            -- §4.1 可选 response_summary：应用层截断到 MICRO_RESPONSE_SUMMARY_MAX_CHARS
            response_summary TEXT,
            -- §4.1 duration_seconds：Micro 独立计时，绝不进 study_sessions
            duration_seconds INTEGER NOT NULL DEFAULT 0,
            completed_at     TEXT NOT NULL DEFAULT (datetime('now')),
            created_at       TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK (source_type IN ('evaluation','learning_item','task','session','goal','none')),
            CHECK (action_type IN ('recall','self_explain','retry_recent_error','review_recent_concept')),
            CHECK (result IN ('done','partial','skipped')),
            CHECK (duration_seconds >= 0),
            CHECK (source_type <> 'none' OR source_id IS NULL)
        );

        -- recent_micro_actions（§4.2/§4.3）：按档案 + 完成时间倒序取最近 N 条
        CREATE INDEX IF NOT EXISTS idx_micro_events_profile_time
            ON micro_learning_events(profile_id, completed_at DESC);

        -- candidate dedupe / pack dedupe（§4.3、DE009）：
        -- 「同一来源 + 同一动作类型 + 时间窗内是否刚做过」必须一次索引命中
        CREATE INDEX IF NOT EXISTS idx_micro_events_dedupe
            ON micro_learning_events(profile_id, source_type, source_id, action_type, completed_at DESC);

        PRAGMA foreign_key_check;",
    )
}
