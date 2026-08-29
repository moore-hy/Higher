//! Sync Wire Model（DEV-SYNC-001 §八）。
//!
//! 跨设备传输的数据结构。铁律：
//! - wire payload **禁止**使用 local database id 作为实体身份，一律 sync_id；
//! - 外键引用一律传输 sync_id（profile_sync_id / goal_sync_id / ...），
//!   接收方经 sync_entity_map 映射为自己的 local id；
//! - settings / search_index / vault / API key 等永远不进入任何 Packet。

use serde::{Deserialize, Serialize};

/// sync_entity_map.entity_type 允许值（v028 CHECK 约束一致）。
pub const ENTITY_STUDY_PROFILE: &str = "study_profile";
pub const ENTITY_GOAL: &str = "goal";
pub const ENTITY_LEARNING_ITEM: &str = "learning_item";
pub const ENTITY_TASK: &str = "task";

pub const OP_UPSERT: &str = "upsert";
pub const OP_DELETE: &str = "delete";

/// 单条变更（outbox 条目 + 实体当前状态快照）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncChange {
    /// 发送方 outbox id；Bootstrap 快照条目固定为 0（不参与增量游标）。
    pub change_id: i64,
    pub entity_type: String,
    pub sync_id: String,
    /// "upsert" | "delete"
    pub operation: String,
    /// upsert 时必有；delete 时为 None。
    pub payload: Option<SyncEntityPayload>,
    pub changed_at: String,
}

/// 四类实体 payload（serde tag "kind" 与 entity_type 值一一对应）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncEntityPayload {
    StudyProfile(StudyProfilePayload),
    Goal(GoalPayload),
    LearningItem(LearningItemPayload),
    Task(TaskPayload),
}

impl SyncEntityPayload {
    pub fn entity_type(&self) -> &'static str {
        match self {
            Self::StudyProfile(_) => ENTITY_STUDY_PROFILE,
            Self::Goal(_) => ENTITY_GOAL,
            Self::LearningItem(_) => ENTITY_LEARNING_ITEM,
            Self::Task(_) => ENTITY_TASK,
        }
    }
}

/// StudyProfile：不含 last_opened_at / active_profile_id（本机状态，不同步）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyProfilePayload {
    pub name: String,
    pub profile_type: Option<String>,
    pub target_description: Option<String>,
    pub target_date: Option<String>,
    pub current_situation: Option<String>,
    pub notes: Option<String>,
    pub status: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalPayload {
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub goal_level: String,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub sort_order: i64,
    pub goal_brief_json: Option<String>,
    pub day_kind: String,
    pub created_at: String,
    pub updated_at: String,
    // ---- 引用（sync_id，非 local id） ----
    pub profile_sync_id: Option<String>,
    pub parent_goal_sync_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningItemPayload {
    pub name: String,
    pub description: Option<String>,
    pub mastery_status: String,
    pub content: String,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    // ---- 引用（sync_id，非 local id） ----
    pub profile_sync_id: Option<String>,
    pub goal_sync_id: Option<String>,
    pub parent_learning_item_sync_id: Option<String>,
}

/// Task：plan_id / recurring_rule_id / planning_blueprint_id / planning_phase_id
/// 属后续 Planning Sync，第一版不跨设备恢复（远端置 NULL），保留 origin / projection_key。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPayload {
    pub title: String,
    pub planned_date: Option<String>,
    pub planned_time: Option<String>,
    pub status: String,
    pub archived_at: Option<String>,
    pub estimated_minutes: Option<i64>,
    pub task_kind: String,
    pub priority: String,
    pub origin: String,
    pub projection_key: String,
    pub user_modified_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    // ---- 引用（sync_id，非 local id） ----
    pub profile_sync_id: Option<String>,
    pub goal_sync_id: Option<String>,
    pub learning_item_sync_id: Option<String>,
}

/// LAN 线协议消息（length-prefixed JSON，单条上限 10 MB）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireMessage {
    /// 客户端配对请求：一次性高熵 pairing_token（DEV-SYNC-003 QR 模式）+ 本机设备信息。
    PairRequest {
        client_device_id: String,
        client_name: String,
        client_platform: String,
        /// DEV-SYNC-003 §四：替代旧 6 位数字码的真正认证凭据（uuid v4，10 分钟一次性）。
        pairing_token: String,
        /// DEV-SYNC-002 §七：客户端监听地址（ip:port）——对端此后可主动反向连接，
        /// 使两端「立即同步」都能触发双向交换。None = 本端未监听。
        #[serde(default)]
        client_listen_addr: Option<String>,
    },
    /// 服务端（Windows）配对应答：成功携带 shared_token + Bootstrap 快照
    ///（当前 Active Profile + goal tree + learning item tree + tasks）。
    PairResponse {
        ok: bool,
        server_device_id: String,
        server_name: String,
        server_platform: String,
        #[serde(default)]
        shared_token: String,
        #[serde(default)]
        bootstrap: Vec<SyncChange>,
        #[serde(default)]
        reason: Option<String>,
    },
    /// 增量同步请求：推送本地 changes + acked_change_id
    ///（= 请求方已消费的对端 outbox 游标，见 sync_peers.last_received_remote_change_id）。
    SyncRequest {
        device_id: String,
        token: String,
        acked_change_id: i64,
        changes: Vec<SyncChange>,
        /// DEV-SYNC-002 §七：客户端监听地址（ip:port），服务端记入 peer_addr。
        #[serde(default)]
        client_listen_addr: Option<String>,
    },
    /// 增量同步应答：应用结果 + acked_change_id
    ///（= 服务端已消费的客户端 outbox 游标）+ 服务端待下发 changes。
    SyncResponse {
        ok: bool,
        acked_change_id: i64,
        changes: Vec<SyncChange>,
        #[serde(default)]
        applied: u32,
        #[serde(default)]
        conflicts: u32,
        #[serde(default)]
        deferred: u32,
        /// DEV-SYNC-002：服务端应用本批客户端变更的明细（per-entity 计数）。
        #[serde(default)]
        outcome: crate::sync::apply::ApplyOutcome,
        #[serde(default)]
        reason: Option<String>,
    },
    /// DEV-SYNC-003-F3 §四：同一 TCP 连接内的反向 ACK —— 发起方应用完对端下发的
    /// changes 后立即回执，使对端 outbox 游标推进 + trim 在本 session 内完成
    ///（否则对端 pending 残留到下一次同步，「一次点击=一次双向收敛」不成立）。
    SyncAck {
        device_id: String,
        token: String,
        /// 发起方已消费的对端 outbox 游标（= 本次 apply 的最大 change_id）。
        acked_change_id: i64,
    },
    SyncAckResponse {
        ok: bool,
        #[serde(default)]
        reason: Option<String>,
    },
    Error {
        message: String,
    },
}
