//! DEV-0061R §18 · Semantic Contract 唯一事实源。
//!
//! - Rust enum / payload 类型在 `action.rs`；本文件只负责：
//!   `SEMANTIC_CONTRACT_VERSION` + Canonical JSON examples + Prompt fragment
//!   + Contract validation helpers。
//! - runtime.rs 的 Prompt **必须引用**本文件的 examples（§18.1 禁止第二份）。
//! - Contract tests（batch061r R05/R06）**必须直接 parse** 本文件 examples（§18.2）。
//! - Skill Markdown 不再复制完整 JSON Schema（§18.3）。
//!
//! v2（DEV-0061R §16）：废弃 `#[serde(flatten)]`——Update 系一律显式 `patch` 字段；
//! patch 缺失/为空 ≠ NothingToChange，而是 ContractFailure（§20）。

use crate::ai::action::SemanticAction;

pub const SEMANTIC_CONTRACT_VERSION: &str = "2";

/// Canonical JSON examples（每行一个完整 SemanticAction；与 Parser 逐字一致）。
/// §31：日期语义统一 TemporalIntent——today/tomorrow/weekday/absolute/offset_days(含负)。
pub const CANONICAL_EXAMPLES: &[&str] = &[
    // ---- Create（Occurrence / Series）----
    r#"{"type":"create_task","title":"背10个英语单词","date":{"kind":"today"},"time_of_day":null,"estimated_minutes":30}"#,
    r#"{"type":"create_recurring_task","title":"背10个英语单词","recurrence":{"kind":"daily"},"start":{"kind":"tomorrow"},"time_of_day":"20:00","estimated_minutes":30}"#,
    // ---- Task Occurrence（target + 显式 patch）----
    r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"背单词","date":{"kind":"today"},"quantity":"singular"},"patch":{"estimated_minutes":30}}"#,
    r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"刚才那个","recency_hint":"recent_created"},"patch":{"planned_time":"21:00"}}"#,
    r#"{"type":"update_task","target":{"entity_type":"task","title_hint":"刚才那个","recency_hint":"recent_created"},"patch":{"planned_date":{"kind":"offset_days","days":2}}}"#,
    r#"{"type":"set_task_status","target":{"entity_type":"task","title_hint":"408","date":{"kind":"today"}},"status":"completed"}"#,
    r#"{"type":"delete_task","target":{"entity_type":"task","title_hint":"408","date":{"kind":"today"},"scope_hint":"occurrence"}}"#,
    // ---- RecurringRule Series（target + patch + reconcile_future）----
    r#"{"type":"update_recurring_task","target":{"entity_type":"recurring_rule","title_hint":"学408"},"patch":{"time_of_day":"21:00","estimated_minutes":45},"reconcile_future":true}"#,
    r#"{"type":"update_recurring_task","target":{"entity_type":"recurring_rule","title_hint":"背单词"},"patch":{"estimated_minutes":20},"reconcile_future":false}"#,
    r#"{"type":"set_recurring_enabled","target":{"entity_type":"recurring_rule","title_hint":"背单词"},"enabled":false}"#,
    r#"{"type":"delete_recurring_rule","target":{"entity_type":"recurring_rule","title_hint":"背单词"}}"#,
    // ---- Bulk（filter + patch）----
    r#"{"type":"bulk_update_tasks","filter":{"date":{"kind":"today"},"status":"not_completed"},"patch":{"planned_date":{"kind":"tomorrow"}}}"#,
];

/// 解析单条 Canonical JSON（与 runtime parse 同入口语义：剥 ```json 围栏）。
pub fn parse_example(raw: &str) -> Option<SemanticAction> {
    let t = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(t).ok()
}

/// 全部 Canonical examples 必须可被 Parser 读取（batch061r R05 / §18.2）。
pub fn all_examples_parse() -> bool {
    CANONICAL_EXAMPLES.iter().all(|e| parse_example(e).is_some())
}

/// Prompt 里的 Contract 说明块（runtime.rs semantic prompt 引用；唯一来源）。
/// 只描述协议形状，不包含业务语义（业务语义在 SKILL.md）。
pub fn prompt_fragment() -> String {
    let examples = CANONICAL_EXAMPLES.join("\n");
    format!(
        "【Semantic Contract v{SEMANTIC_CONTRACT_VERSION}】\n\
输出一个 JSON 对象（tag type=snake_case）。字段协议：\n\
- create_task / create_recurring_task：直接携带 title/date(time)/recurrence/start 等创建字段。\n\
- update_task / update_recurring_task：必须形如 {{\"type\":…,\"target\":{{…}},\"patch\":{{…}}}}。\
修改字段只放进 patch（如 estimated_minutes / planned_time / planned_date / time_of_day / title）。\
patch 只含用户真正要求修改的字段；绝不允许顶层平铺修改字段或使用 \"update\" 键。\n\
- bulk_update_tasks：{{\"filter\":{{\"date\":…,\"status\":\"not_completed\"}},\"patch\":{{…}}}}。\n\
- set_task_status / delete_task / set_recurring_enabled / delete_recurring_rule：target + 动作字段。\n\
- target 是 ReferenceHint（用户说的是谁）：entity_type/title_hint/date/status_hint/recurrence_hint/\
recency_hint（\"刚才那个\"→\"recent_created\"）/quantity（singular|plural）。禁止输出任何数据库 id。\n\
- 日期统一 {{kind}}：today / tomorrow / yesterday / weekday（weekday 1..7 周一起算）/ \
absolute_date / offset_days（days 可为负，如 -1=昨天、2=后天）。\n\
Canonical 示例：\n{examples}"
    )
}

/// Repair 用最小 Contract 块（§19：Repair 输入只含 Canonical Contract + invalid JSON + parser error）。
pub fn repair_instruction(invalid_json: &str, parser_error: &str) -> String {
    format!(
        "下面这段输出不符合 Semantic Contract v{SEMANTIC_CONTRACT_VERSION}。\n\
错误摘要：{parser_error}\n\
{contract}\n\
【无效输出】\n{invalid_json}\n\
只修复 JSON 结构（字段名/嵌套/枚举值），不要新增、删除或改变任何语义字段，\
重新输出一个完整 JSON（不要 markdown 代码块、不要解释）。",
        contract = prompt_fragment(),
    )
}
