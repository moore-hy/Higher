//! DEV-0077.4-A.1 · Learning Grounding — 类型定义（§十-§十四/§四〇/§九四）。
//!
//! 概念冻结（§六）：LearningItem = 可重复学习/验证/掌握的知识或技能单位；
//! Task = 一次具体学习行为。禁止把「某日学习90分钟」这类活动当 Unit。
//!
//! ref_key（§十一）：只在当前 PlanDraft / ChangeSet 内使用的草稿局部引用，
//! **不是数据库永久 ID**；Compiler 期 → LearningItem 真实 id（复用）或
//! 同包 knowledge create 的 operation_ref（新建）。

use std::collections::HashMap;

/// §十：Planner 输出的学习单元草稿。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LearningUnitDraft {
    /// §十一：Draft 内唯一引用键（如 math / math.calculus / cs408.list）。
    pub ref_key: String,
    /// 知识/技能单位名（如「极限」「线性表」）；禁止日期型活动名（§六）。
    pub name: String,
    /// 父单元 ref_key；空 = Root（§二十四）。
    #[serde(default)]
    pub parent_ref: String,
    #[serde(default)]
    pub description: Option<String>,
    /// §五十五：可选 Goal 关联（最相关 Formal Goal 的 ref；不破坏可复用性）。
    #[serde(default)]
    pub goal_ref: Option<String>,
    /// §二十.1：可信已有 LearningItem id（Planner 上下文注入；验证后直接复用）。
    #[serde(default)]
    pub existing_learning_item_id: Option<i64>,
}

/// §十二/§十六：Task Grounding 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskGroundingMode {
    /// 学习任务：恰好 1 个 Primary Learning Unit（§十四原子性）。
    Learning,
    /// 杂务（整理资料/周复盘/报名检查…）：合法无知识关联（§十六），
    /// 区别于「应该关联但丢失」。
    Meta,
}

impl TaskGroundingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Learning => "learning",
            Self::Meta => "meta",
        }
    }
}

/// §十二：Planner Task 的结构化 Grounding。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskGroundingDraft {
    pub mode: TaskGroundingMode,
    /// §十三：Vec 是为了让 Validator 能发现「一个 Task 塞多个学习活动」；
    /// 正式规则 Learning==1 / Meta==0，>1 → INVALID，要求拆 Task。
    #[serde(default)]
    pub unit_refs: Vec<String>,
    #[serde(default)]
    pub rationale: Option<String>,
}

/// §九十四：Grounding Key = (resolved parent, normalized name)（profile 内）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GroundingKey {
    pub parent_id: Option<i64>,
    pub normalized_name: String,
}

/// 新建单元的父指定（同包 ref 或已有真实 id 或 Root）。
#[derive(Debug, Clone)]
pub enum ParentSpec {
    Root,
    /// 复用已有 LearningItem 作为父（真实 id）。
    Existing(i64),
    /// 同 ChangeSet 内前序 create（ref_key = operation_ref）。
    PackRef(String),
}

/// 待创建的 Unit（已按 parents-first 排序）。
#[derive(Debug, Clone)]
pub struct CreateUnit {
    pub ref_key: String,
    pub name: String,
    pub description: Option<String>,
    pub goal_ref: Option<String>,
    pub parent: ParentSpec,
}

/// §二十：Resolution 结果——复用映射 + 待创建清单。
#[derive(Debug, Clone, Default)]
pub struct GroundingResolution {
    /// ref_key → 已有 LearningItem 真实 id（复用；不产生任何 op）。
    pub reuse: HashMap<String, i64>,
    /// 需在同 ChangeSet 内新建的单元（父先子后）。
    pub create: Vec<CreateUnit>,
}

impl GroundingResolution {
    /// Task 引用单元 → op after 注入值：复用 → (learning_item_id, real)；
    /// 新建 → (learning_item_ref, ref_key)。
    pub fn task_item_binding(&self, unit_ref: &str) -> Option<(&'static str, serde_json::Value)> {
        if let Some(id) = self.reuse.get(unit_ref) {
            return Some(("learning_item_id", serde_json::json!(id)));
        }
        if self.create.iter().any(|c| c.ref_key == unit_ref) {
            return Some(("learning_item_ref", serde_json::json!(unit_ref)));
        }
        None
    }
}

/// §四〇：Grounding Completeness 诊断（完整规划 rate 必须为 1.0）。
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GroundingCompleteness {
    pub learning_task_count: usize,
    pub grounded_learning_task_count: usize,
    pub meta_task_count: usize,
    pub invalid_unlinked_learning_task_count: usize,
    /// grounded / learning（learning_task_count == 0 时为 1.0——无学习任务不算缺口）。
    pub rate: f64,
}
