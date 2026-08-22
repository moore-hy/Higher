//! DEV-0060.1 PART M · Performance Trace（复用 ai_run_events；0 migration）。
//!
//! 事件：route_decided / context_built / provider_request_started / provider_first_delta /
//! provider_request_finished / tool_round_started / tool_round_finished /
//! semantic_action_parsed / domain_resolved / changeset_compiled / run_finished。
//! data_json：route / duration_ms / provider_request_index / tool_count / tool_round /
//! context_chars / skill_ids / provider_call_kind(main|secondary)。
//! 永不记录：API Key / 完整 PersonalProfile / 完整 Prompt / 用户隐私全文（§24.1）。

use rusqlite::{params, Connection};
use serde_json::json;

pub struct Trace {
    pub run_id: String,
    started: std::time::Instant,
    /// main provider requests（用户可见回答）
    pub main_requests: i64,
    /// secondary requests（router/semantic/repair/memory extract 等）
    pub secondary_requests: i64,
    first_delta_recorded: bool,
}

impl Trace {
    pub fn new(run_id: &str) -> Self {
        Self {
            run_id: run_id.to_string(),
            started: std::time::Instant::now(),
            main_requests: 0,
            secondary_requests: 0,
            first_delta_recorded: false,
        }
    }

    fn emit(&self, conn: &Connection, event: &str, mut data: serde_json::Value) {
        if let Some(o) = data.as_object_mut() {
            o.insert("t_ms".into(), json!(self.started.elapsed().as_millis() as i64));
        }
        let _ = conn.execute(
            "INSERT INTO ai_run_events (run_id, event_type, data_json) VALUES (?1, ?2, ?3)",
            params![self.run_id, event, data.to_string()],
        );
    }

    pub fn route_decided(&self, conn: &Connection, route: &str, how: &str, skills: &[String]) {
        self.emit(conn, "route_decided", json!({ "route": route, "decided_by": how, "skill_ids": skills }));
    }

    /// DEV-0061R §43：turn 开始（page label；safe metadata only）。
    pub fn turn_started(&self, conn: &Connection, page_label: &str) {
        self.emit(conn, "turn_started", json!({ "page_label": page_label }));
    }

    /// DEV-0061R §43：TurnDecision 产出（与 route_decided 并存：route_decided 保留兼容）。
    pub fn turn_decided(&self, conn: &Connection, decision: &str, how: &str) {
        self.emit(conn, "turn_decided", json!({ "decision": decision, "decided_by": how }));
    }

    /// DEV-0061R §19：Repair Once 发生记录。
    pub fn semantic_action_repaired(&self, conn: &Connection, outcome: &str) {
        self.emit(conn, "semantic_action_repaired", json!({ "outcome": outcome }));
    }

    /// DEV-0061R §43：ChangeSet 创建成功记录。
    pub fn changeset_created(&self, conn: &Connection, change_set_id: i64, op_count: usize) {
        self.emit(conn, "changeset_created", json!({
            "change_set_id": change_set_id, "op_count": op_count }));
    }

    pub fn context_built(&self, conn: &Connection, context_chars: usize, chips: &[String]) {
        self.emit(conn, "context_built", json!({ "context_chars": context_chars, "chips": chips }));
    }

    pub fn provider_request_started(&self, conn: &Connection, index: i64, kind: &str, tools: usize) {
        self.emit(conn, "provider_request_started", json!({
            "provider_request_index": index, "provider_call_kind": kind, "tool_count": tools }));
    }

    pub fn provider_first_delta(&mut self, conn: &Connection, index: i64) {
        if !self.first_delta_recorded {
            self.first_delta_recorded = true;
            self.emit(conn, "provider_first_delta", json!({ "provider_request_index": index }));
        }
    }

    pub fn provider_request_finished(&mut self, conn: &Connection, index: i64, kind: &str) {
        match kind {
            "main" => self.main_requests += 1,
            _ => self.secondary_requests += 1,
        }
        self.emit(conn, "provider_request_finished", json!({
            "provider_request_index": index,
            "provider_call_kind": kind,
            "main_total": self.main_requests,
            "secondary_total": self.secondary_requests,
        }));
    }

    pub fn tool_round(&self, conn: &Connection, round: usize, tool_count: usize) {
        self.emit(conn, "tool_round_started", json!({ "tool_round": round, "tool_count": tool_count }));
        self.emit(conn, "tool_round_finished", json!({ "tool_round": round }));
    }

    pub fn semantic_action_parsed(&self, conn: &Connection, action_type: &str) {
        self.emit(conn, "semantic_action_parsed", json!({ "action_type": action_type }));
    }

    pub fn domain_resolved(&self, conn: &Connection, outcome: &str) {
        self.emit(conn, "domain_resolved", json!({ "outcome": outcome }));
    }

    pub fn changeset_compiled(&self, conn: &Connection, ops: usize) {
        self.emit(conn, "changeset_compiled", json!({ "op_count": ops }));
    }

    // ---- DEV-0060.2 PART R · Grounding / ActionPlan 事件（只记计数与路由，不记 payload） ----

    pub fn grounding_started(&self, conn: &Connection, entity_type: &str) {
        self.emit(conn, "grounding_started", json!({ "entity_type": entity_type }));
    }

    pub fn candidates_retrieved(&self, conn: &Connection, entity_type: &str, candidate_count: usize) {
        self.emit(conn, "candidates_retrieved", json!({
            "entity_type": entity_type, "candidate_count": candidate_count }));
    }

    pub fn grounding_resolved(&self, conn: &Connection, entity_type: &str, selection_provider_called: bool) {
        self.emit(conn, "grounding_resolved", json!({
            "entity_type": entity_type, "selection_provider_called": selection_provider_called }));
    }

    pub fn grounding_ambiguous(&self, conn: &Connection, candidate_count: usize) {
        self.emit(conn, "grounding_ambiguous", json!({ "candidate_count": candidate_count }));
    }

    pub fn grounding_not_found(&self, conn: &Connection, entity_type: &str) {
        self.emit(conn, "grounding_not_found", json!({ "entity_type": entity_type }));
    }

    pub fn candidate_selection_started(&self, conn: &Connection, candidate_count: usize) {
        self.emit(conn, "candidate_selection_started", json!({ "candidate_count": candidate_count }));
    }

    pub fn candidate_selection_finished(&self, conn: &Connection, result: &str) {
        self.emit(conn, "candidate_selection_finished", json!({ "result": result }));
    }

    pub fn action_plan_compiled(&self, conn: &Connection, operation_count: usize) {
        self.emit(conn, "action_plan_compiled", json!({ "operation_count": operation_count }));
    }

    pub fn empty_plan_guarded(&self, conn: &Connection, reason: &str) {
        self.emit(conn, "empty_plan_guarded", json!({ "reason": reason }));
    }

    pub fn run_finished(&self, conn: &Connection, status: &str) {
        self.emit(
            conn,
            "run_finished",
            json!({
                "status": status,
                "duration_ms": self.started.elapsed().as_millis() as i64,
                "main_provider_requests": self.main_requests,
                "secondary_provider_requests": self.secondary_requests,
            }),
        );
    }
}
