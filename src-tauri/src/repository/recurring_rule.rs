use rusqlite::{params, Connection};

/// 重复任务规则（BATCH-04 / v013 起 Profile First）。
///
/// - repeat_type：daily / weekly（weekly 用 weekdays_json: [1,3,5]，周一=1…周日=7）
/// - **profile_id NOT NULL 直挂**；goal_id / learning_item_id 均可空（"每天背单词"无需 Goal）
/// - time_of_day：Higher 内的任务出现时间（通知由 DEV-0042 插件处理）
/// - 修改规则只影响未来 materialization，不重写历史 Task
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RecurringRule {
    pub id: i64,
    pub profile_id: i64,
    #[serde(default)]
    pub goal_id: Option<i64>,
    #[serde(default)]
    pub learning_item_id: Option<i64>,
    pub title: String,
    pub repeat_type: String,
    pub weekdays_json: String,
    pub time_of_day: Option<String>,
    pub start_date: String,
    pub end_date: Option<String>,
    pub enabled: bool,
    /// DEV-0060.1 v023：未来 materialized Task 继承（NULL=未设置）
    #[serde(default)]
    pub estimated_minutes: Option<i64>,
    /// DEV-0060.1 v023：structured | accumulation
    #[serde(default = "default_task_kind")]
    pub task_kind: String,
    /// DEV-0060.1 v023：core | normal
    #[serde(default = "default_priority")]
    pub priority: String,
    pub created_at: String,
    pub updated_at: String,
}

fn default_task_kind() -> String {
    "structured".into()
}

fn default_priority() -> String {
    "normal".into()
}

/// v023 三字段的可选覆盖（None=保留默认/现状；Some("")=非法）。
#[derive(Debug, Default, Clone)]
pub struct RuleSemantics {
    pub estimated_minutes: Option<i64>,
    pub task_kind: Option<String>,
    pub priority: Option<String>,
}

impl RuleSemantics {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(m) = self.estimated_minutes {
            if !(1..=1440).contains(&m) {
                return Err(format!("预计学习分钟必须在 1~1440 之间（收到 {m}）"));
            }
        }
        if let Some(k) = &self.task_kind {
            if k != "structured" && k != "accumulation" {
                return Err(format!("task_kind 非法：{k}"));
            }
        }
        if let Some(p) = &self.priority {
            if p != "core" && p != "normal" {
                return Err(format!("priority 非法：{p}"));
            }
        }
        Ok(())
    }
    pub fn kind_or(&self, d: &str) -> &str {
        match &self.task_kind {
            Some(k) => k.as_str(),
            None => {
                if d == "accumulation" {
                    "accumulation"
                } else {
                    "structured"
                }
            }
        }
    }
    pub fn priority_or(&self, d: &str) -> &str {
        match &self.priority {
            Some(p) => p.as_str(),
            None => {
                if d == "core" {
                    "core"
                } else {
                    "normal"
                }
            }
        }
    }
}

const RULE_COLUMNS: &str = "id, profile_id, goal_id, learning_item_id, title, repeat_type, weekdays_json, time_of_day, start_date, end_date, enabled, estimated_minutes, task_kind, priority, created_at, updated_at";

pub struct RecurringRuleRepository<'a> {
    conn: &'a Connection,
}

impl<'a> RecurringRuleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// 创建规则（Profile First：profile_id 必填；goal/item 可空；weekly 至少一个星期）。
    /// DEV-0060.1 v023：支持 estimated_minutes/task_kind/priority（RuleSemantics）。
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        repeat_type: &str,
        weekdays: &[u32],
        time_of_day: Option<&str>,
        start_date: &str,
        end_date: Option<&str>,
    ) -> Result<RecurringRule, String> {
        self.create_with_semantics(
            profile_id, goal_id, learning_item_id, title, repeat_type, weekdays, time_of_day,
            start_date, end_date, &RuleSemantics::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_with_semantics(
        &self,
        profile_id: i64,
        goal_id: Option<i64>,
        learning_item_id: Option<i64>,
        title: &str,
        repeat_type: &str,
        weekdays: &[u32],
        time_of_day: Option<&str>,
        start_date: &str,
        end_date: Option<&str>,
        semantics: &RuleSemantics,
    ) -> Result<RecurringRule, String> {
        validate(repeat_type, weekdays, start_date, end_date)?;
        semantics.validate()?;
        self.conn
            .execute(
                "INSERT INTO recurring_task_rules
                 (profile_id, goal_id, learning_item_id, title, repeat_type, weekdays_json, time_of_day, start_date, end_date,
                  estimated_minutes, task_kind, priority)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    profile_id,
                    goal_id,
                    learning_item_id,
                    title,
                    repeat_type,
                    serde_json::to_string(weekdays).unwrap_or_else(|_| "[]".into()),
                    time_of_day,
                    start_date,
                    end_date,
                    semantics.estimated_minutes,
                    semantics.kind_or("structured"),
                    semantics.priority_or("normal"),
                ],
            )
            .map_err(|e| e.to_string())?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or_else(|| "规则创建失败".to_string())
    }

    pub fn get(&self, id: i64) -> Result<Option<RecurringRule>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM recurring_task_rules WHERE id = ?1",
                RULE_COLUMNS
            ))
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query_map(params![id], |r| parse_rule(r))
            .map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 档案内全部规则（含停用；管理列表用；Profile 直查）。
    pub fn list_by_profile(&self, profile_id: i64) -> Result<Vec<RecurringRule>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {} FROM recurring_task_rules r
                 WHERE r.profile_id = ?1
                 ORDER BY r.id DESC",
                RULE_COLUMNS
                    .split(", ")
                    .map(|c| format!("r.{}", c))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |r| parse_rule(r))
            .map_err(|e| e.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
    }

    /// 编辑规则（只影响未来 materialization，不重写历史 Task）。
    /// DEV-0060.1 v023：update_with_semantics 可同步改三字段（None=保留现状）。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: i64,
        title: &str,
        repeat_type: &str,
        weekdays: &[u32],
        time_of_day: Option<&str>,
        start_date: &str,
        end_date: Option<&str>,
        learning_item_id: Option<i64>,
    ) -> Result<(), String> {
        self.update_with_semantics(
            id, title, repeat_type, weekdays, time_of_day, start_date, end_date, learning_item_id,
            &RuleSemantics::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_with_semantics(
        &self,
        id: i64,
        title: &str,
        repeat_type: &str,
        weekdays: &[u32],
        time_of_day: Option<&str>,
        start_date: &str,
        end_date: Option<&str>,
        learning_item_id: Option<i64>,
        semantics: &RuleSemantics,
    ) -> Result<(), String> {
        validate(repeat_type, weekdays, start_date, end_date)?;
        semantics.validate()?;
        // 同档案校验（learning_item 若存在必须属于规则所在档案）
        if let Some(item) = learning_item_id {
            let profile_of_rule: Option<i64> = self
                .conn
                .query_row(
                    "SELECT profile_id FROM recurring_task_rules WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .ok();
            let profile_of_item: Option<i64> = self
                .conn
                .query_row(
                    "SELECT profile_id FROM learning_items WHERE id = ?1",
                    params![item],
                    |r| r.get(0),
                )
                .ok();
            match (profile_of_rule, profile_of_item) {
                (Some(a), Some(b)) if a == b => {}
                _ => return Err("所选知识不属于当前学习档案，无法关联".to_string()),
            }
        }
        let existing = self.get(id)?.ok_or("规则不存在")?;
        self.conn
            .execute(
                "UPDATE recurring_task_rules
                 SET title=?1, repeat_type=?2, weekdays_json=?3, time_of_day=?4,
                     start_date=?5, end_date=?6, learning_item_id=?7,
                     estimated_minutes=?8, task_kind=?9, priority=?10, updated_at=datetime('now')
                 WHERE id = ?11",
                params![
                    title,
                    repeat_type,
                    serde_json::to_string(weekdays).unwrap_or_else(|_| "[]".into()),
                    time_of_day,
                    start_date,
                    end_date,
                    learning_item_id,
                    semantics.estimated_minutes.or(existing.estimated_minutes),
                    semantics.kind_or(&existing.task_kind),
                    semantics.priority_or(&existing.priority),
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_enabled(&self, id: i64, enabled: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE recurring_task_rules SET enabled = ?1, updated_at=datetime('now') WHERE id = ?2",
                params![if enabled { 1 } else { 0 }, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 删除规则（历史已生成 Task 保留：tasks.recurring_rule_id 无 FK，仅标记）。
    pub fn delete(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM recurring_task_rules WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn validate(
    repeat_type: &str,
    weekdays: &[u32],
    start_date: &str,
    end_date: Option<&str>,
) -> Result<(), String> {
    if repeat_type != "daily" && repeat_type != "weekly" {
        return Err("重复类型仅支持：每天 / 每周".to_string());
    }
    if repeat_type == "weekly" && weekdays.is_empty() {
        return Err("每周重复需要至少选择一个星期".to_string());
    }
    if weekdays.iter().any(|&d| !(1..=7).contains(&d)) {
        return Err("星期取值非法".to_string());
    }
    if start_date.trim().is_empty() {
        return Err("开始日期不能为空".to_string());
    }
    if let Some(end) = end_date {
        if !end.trim().is_empty() && end < start_date {
            return Err("结束日期不能早于开始日期".to_string());
        }
    }
    Ok(())
}

fn parse_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecurringRule> {
    Ok(RecurringRule {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        goal_id: row.get(2)?,
        learning_item_id: row.get(3)?,
        title: row.get(4)?,
        repeat_type: row.get(5)?,
        weekdays_json: row.get(6)?,
        time_of_day: row.get(7)?,
        start_date: row.get(8)?,
        end_date: row.get(9)?,
        enabled: row.get::<_, i64>(10)? != 0,
        estimated_minutes: row.get(11)?,
        task_kind: row.get(12)?,
        priority: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

/// 指定日期是否命中规则（daily：每天；weekly：星期匹配）。
pub fn rule_matches_date(rule: &RecurringRule, date: &str) -> bool {
    if !rule.enabled {
        return false;
    }
    if date < rule.start_date.as_str() {
        return false;
    }
    if let Some(end) = &rule.end_date {
        if !end.is_empty() && date > end.as_str() {
            return false;
        }
    }
    match rule.repeat_type.as_str() {
        "daily" => true,
        "weekly" => {
            let weekdays: Vec<u32> =
                serde_json::from_str(&rule.weekdays_json).unwrap_or_default();
            weekday_of(date).map(|w| weekdays.contains(&w)).unwrap_or(false)
        }
        _ => false,
    }
}

/// 'YYYY-MM-DD' → 星期（周一=1 … 周日=7）。纯日期算术（无 chrono 依赖）。
pub fn weekday_of(date: &str) -> Option<u32> {
    let parts: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    if parts.len() != 3 {
        return None;
    }
    let (y, m, d) = (parts[0], parts[1], parts[2]);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Sakamoto 算法（civil from days）
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut yy = y;
    if m < 3 {
        yy -= 1;
    }
    let w = (yy + yy / 4 - yy / 100 + yy / 400 + t[(m - 1) as usize] + d) % 7;
    // Sakamoto: 0=Sunday → 转周一=1…周日=7
    Some(if w == 0 { 7 } else { w as u32 })
}

/// Materialization（DEV-0026 §62-63）：为档案内所有启用规则在 date 当天生成 Task。
///
/// 幂等：同 (rule_id, planned_date) 最多 1 Task——已存在即跳过（刷新/重启/重复调用安全）。
/// 返回新创建 Task 数。
pub fn materialize_recurring_tasks(
    conn: &Connection,
    profile_id: i64,
    date: &str,
) -> Result<i64, String> {
    let rules = RecurringRuleRepository::new(conn).list_by_profile(profile_id)?;
    let task_repo = super::task::TaskRepository::new(conn);
    let mut created = 0i64;
    for rule in rules {
        if !rule_matches_date(&rule, date) {
            continue;
        }
        if task_repo
            .exists_for_rule_date(rule.id, date)
            .map_err(|e| e.to_string())?
        {
            continue; // 幂等
        }
        task_repo
            .create_from_rule_v2(
                rule.profile_id,
                rule.goal_id,
                rule.learning_item_id,
                &rule.title,
                date,
                rule.time_of_day.as_deref(),
                rule.id,
                rule.estimated_minutes,
                &rule.task_kind,
                &rule.priority,
            )
            .map_err(|e| e.to_string())?;
        created += 1;
    }
    Ok(created)
}

// =============== DEV-0061R §51-54 · Bounded Materialization（Range + Rolling Horizon） ===============

/// 固定 Rolling Horizon（§52）：新 Rule Apply 后 / Today 刷新时保证未来 30 天可见。
pub const ROLLING_HORIZON_DAYS: i64 = 30;
/// Range 上限防御（§54 bounded；Calendar 一次最多一个月，400 天绝对富余）。
const MAX_RANGE_DAYS: i64 = 400;

/// 纯日期 +n（SQLite julianday；无时区语义）。pub：DEV-0062 action_continuation 相对日解析复用。
pub fn shift_date(base: &str, n: i64) -> Result<String, String> {
    Connection::open_in_memory()
        .and_then(|c| {
            c.query_row(
                "SELECT date(?1, printf('%+d days', ?2))",
                params![base, n],
                |r| r.get::<_, String>(0),
            )
        })
        .map_err(|e| format!("日期运算失败：{e}"))
}

/// 范围内有界物化（§54：idempotent / deterministic / bounded；重复调用 0 duplicate）。
/// 覆盖 Planning Calendar 可见月（§53：可超 rolling 30 天，按显示月份有界物化）。
pub fn materialize_recurring_tasks_range(
    conn: &Connection,
    profile_id: i64,
    start_date: &str,
    end_date: &str,
) -> Result<i64, String> {
    let s = shift_date(start_date, 0)?;
    let e = shift_date(end_date, 0)?;
    if e < s {
        return Err(format!("materialize range 起止颠倒：{s}..{e}"));
    }
    let span: i64 = conn
        .query_row(
            "SELECT CAST(julianday(?1) - julianday(?2) AS INTEGER)",
            params![e, s],
            |r| r.get(0),
        )
        .map_err(|er| er.to_string())?;
    if span > MAX_RANGE_DAYS {
        return Err(format!("materialize range 超出有界上限（{span} 天 > {MAX_RANGE_DAYS}）"));
    }
    let mut created = 0i64;
    let mut d = s.clone();
    loop {
        created += materialize_recurring_tasks(conn, profile_id, &d)?;
        if d == e {
            break;
        }
        d = shift_date(&d, 1)?;
    }
    Ok(created)
}

/// Rolling Horizon 物化（§52：today .. today+30）。
pub fn materialize_rolling_horizon(
    conn: &Connection,
    profile_id: i64,
    today: &str,
) -> Result<i64, String> {
    let end = shift_date(today, ROLLING_HORIZON_DAYS)?;
    materialize_recurring_tasks_range(conn, profile_id, today, &end)
}
