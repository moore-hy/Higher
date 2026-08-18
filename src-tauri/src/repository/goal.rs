use rusqlite::{params, Connection};

/// 学习目标（v015 起 = 目标树节点）。
///
/// 同一数据模型四种层级（§16）：goal_level ∈ final | year | month | day（legacy = 历史兼容）。
/// - final：每 Profile 唯一（partial unique index），parent NULL，不可删除
/// - year/month/day：严格逐级 parent（final→year→month→day），period 由层级推导
/// - Goal 树只负责组织方向，不是学习权限门（§21）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Goal {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub profile_id: Option<i64>,
    /// v015：父目标（final=NULL）
    #[serde(default)]
    pub parent_goal_id: Option<i64>,
    /// v015：final | year | month | day | legacy
    #[serde(default = "default_goal_level")]
    pub goal_level: String,
    /// v015：周期起止（YYYY-MM-DD）
    #[serde(default)]
    pub period_start: Option<String>,
    #[serde(default)]
    pub period_end: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    /// v019（DEV-0055 PART 6）：Final Goal Canonical Brief（JSON；仅 final 行使用）
    #[serde(default)]
    pub goal_brief_json: Option<String>,
}

fn default_goal_level() -> String {
    "legacy".into()
}

/// DEV-0055 §19 Generic Goal Brief：通用（非考研专项）七字段结构。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GoalBrief {
    /// 简短标题（如 "2027 考研"）
    #[serde(default)]
    pub title: String,
    /// 最终想实现什么（一句话）
    #[serde(default)]
    pub outcome: String,
    /// YYYY-MM-DD 或 null=明确无 deadline
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    /// 未决事项（Goal 不完整的具体清单）
    #[serde(default)]
    pub unresolved: Vec<String>,
}

impl GoalBrief {
    /// §27 Planning Readiness 最低要求：outcome + (deadline 明确或明确无) + ≥1 success_criteria。
    /// DEV-0058 §30：文案禁内部字段名（用户直接可见）。
    pub fn readiness_missing(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if self.outcome.trim().is_empty() {
            missing.push("最终想达到什么".to_string());
        }
        if self.deadline.is_none() && !self.no_deadline_confirmed() {
            missing.push("截止时间（或确认无截止）".to_string());
        }
        if self.success_criteria.iter().all(|s| s.trim().is_empty()) {
            missing.push("至少一条成功标准".to_string());
        }
        missing
    }

    fn no_deadline_confirmed(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| c.contains("无截止") || c.to_lowercase().contains("no deadline"))
    }

    pub fn is_ready(&self) -> bool {
        self.readiness_missing().is_empty()
    }
}

/// 目标树（§25 文件树结构；legacy 节点单列不混入层级）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GoalTreeNode {
    #[serde(flatten)]
    pub goal: Goal,
    pub children: Vec<GoalTreeNode>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct GoalTree {
    pub final_goal: GoalTreeNode,
    pub legacy_goals: Vec<Goal>,
}

const GOAL_COLS: &str = "id, name, description, status, profile_id, parent_goal_id, goal_level, period_start, period_end, sort_order, created_at, updated_at, goal_brief_json";

pub struct GoalRepository<'a> {
    conn: &'a Connection,
}

impl<'a> GoalRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // =============== 既有接口（保留） ===============

    /// 创建 Goal（旧接口：无层级信息 → legacy；仅测试/兼容用）。
    pub fn create(&self, profile_id: i64, name: &str, description: Option<&str>) -> rusqlite::Result<Goal> {
        self.conn
            .execute(
                "INSERT INTO goals (name, description, profile_id, goal_level) VALUES (?1, ?2, ?3, 'legacy')",
                params![name, description, profile_id],
            )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn list(&self) -> rusqlite::Result<Vec<Goal>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM goals ORDER BY id",
            GOAL_COLS
        ))?;
        let rows = stmt.query_map([], parse_goal)?;
        rows.collect()
    }

    pub fn list_by_profile(&self, profile_id: i64) -> rusqlite::Result<Vec<Goal>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM goals WHERE profile_id = ?1 ORDER BY sort_order, id",
            GOAL_COLS
        ))?;
        let rows = stmt.query_map(params![profile_id], parse_goal)?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Goal>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {} FROM goals WHERE id = ?1", GOAL_COLS))?;
        let mut rows = stmt.query_map(params![id], parse_goal)?;
        rows.next().transpose()
    }

    pub fn belongs_to_profile(&self, goal_id: i64, profile_id: i64) -> rusqlite::Result<bool> {
        let actual: Option<Option<i64>> = self
            .conn
            .query_row(
                "SELECT profile_id FROM goals WHERE id = ?1",
                params![goal_id],
                |row| row.get(0),
            )
            .ok();
        match actual {
            Some(Some(pid)) => Ok(pid == profile_id),
            _ => Ok(false),
        }
    }

    /// 编辑名称 / 描述（不改层级与周期）。
    pub fn update(&self, id: i64, name: &str, description: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE goals SET name = ?1, description = ?2, updated_at = datetime('now')
             WHERE id = ?3",
            params![name, description, id],
        )?;
        Ok(())
    }

    // =============== v019 Final Goal Canonical Brief（DEV-0055 PART 5-6） ===============

    /// §19：读 Final Goal Brief（无/解析失败 → 全空 Default；前端显示"目标待确认"）。
    pub fn get_final_brief(&self, profile_id: i64) -> rusqlite::Result<GoalBrief> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT goal_brief_json FROM goals
                 WHERE profile_id = ?1 AND goal_level = 'final'",
                params![profile_id],
                |r| r.get(0),
            )
            .unwrap_or(None);
        Ok(raw
            .and_then(|s| serde_json::from_str::<GoalBrief>(&s).ok())
            .unwrap_or_default())
    }

    /// §198：写 Brief（用户确认路径：Manual 表单 或 AI 提案 ChangeSet apply 后均走此）。
    /// 校验：id 必须是该 Profile 的 final。
    /// DEV-0057 §27 Canonical Title：brief.title 非空时同事务同步 goals.name（name=显示投影，
    /// 不得成为第二事实源）。
    pub fn set_final_brief(&self, profile_id: i64, brief: &GoalBrief) -> Result<(), String> {
        let json = serde_json::to_string(brief).map_err(|e| e.to_string())?;
        let title = brief.title.trim();
        let n = if title.is_empty() {
            self.conn
                .execute(
                    "UPDATE goals SET goal_brief_json = ?1, updated_at = datetime('now')
                     WHERE profile_id = ?2 AND goal_level = 'final'",
                    params![json, profile_id],
                )
                .map_err(|e| e.to_string())?
        } else {
            self.conn
                .execute(
                    "UPDATE goals SET goal_brief_json = ?1, name = ?3, updated_at = datetime('now')
                     WHERE profile_id = ?2 AND goal_level = 'final'",
                    params![json, profile_id, title],
                )
                .map_err(|e| e.to_string())?
        };
        if n == 0 {
            return Err("该档案还没有最终目标".to_string());
        }
        Ok(())
    }

    /// §16 Goal Conflict Rule：检测目标多源冲突（Canonical Brief vs profile.target_* vs
    /// goals.description vs Personalization）。返回冲突描述列表（空=无冲突）。
    /// **不自动选择**（§169）；调用方（AI/UI）必须提示用户确认。
    pub fn detect_goal_conflicts(&self, profile_id: i64) -> Vec<String> {
        let mut conflicts = Vec::new();
        let brief = self.get_final_brief(profile_id).unwrap_or_default();
        let (prof_desc, prof_date): (Option<String>, Option<String>) = self
            .conn
            .query_row(
                "SELECT target_description, target_date FROM study_profiles WHERE id = ?1",
                params![profile_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((None, None));
        let goal_desc: Option<String> = self
            .conn
            .query_row(
                "SELECT description FROM goals WHERE profile_id = ?1 AND goal_level = 'final'",
                params![profile_id],
                |r| r.get(0),
            )
            .unwrap_or(None);

        // Brief 已有 canonical deadline，而 profile.target_date 是不同日期 → 冲突
        if let (Some(bd), Some(pd)) = (brief.deadline.as_deref(), prof_date.as_deref()) {
            if !bd.is_empty() && !pd.is_empty() && bd != pd {
                conflicts.push(format!(
                    "截止时间存在两个说法：最终目标「{bd}」 vs 档案信息「{pd}」"
                ));
            }
        }
        // Brief 有 outcome，profile.target_description 非空且非同一句 → 疑似不同目标表述
        if !brief.outcome.trim().is_empty() {
            if let Some(d) = prof_desc.as_deref() {
                let d = d.trim();
                if !d.is_empty() && d != brief.outcome.trim() && !d.contains(&brief.outcome) {
                    conflicts.push("目标描述存在两个不同版本（最终目标 vs 档案目标描述）".to_string());
                }
            }
            if let Some(g) = goal_desc.as_deref() {
                let g = g.trim();
                if !g.is_empty() && g != brief.outcome.trim() && !g.contains(&brief.outcome) {
                    conflicts.push("目标描述存在两个不同版本（最终目标 vs 目标备注）".to_string());
                }
            }
        }
        conflicts
    }

    pub fn archive(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE goals SET status = 'archived', updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn restore(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE goals SET status = 'active', updated_at = datetime('now') WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // =============== v015 目标树 ===============

    /// 确保该 Profile 存在唯一 Final（无则创建占位）。新建 Profile 后由 command 层调用。
    pub fn ensure_final(&self, profile_id: i64) -> rusqlite::Result<Goal> {
        if let Some(g) = self.final_of(profile_id)? {
            return Ok(g);
        }
        self.conn.execute(
            "INSERT INTO goals (profile_id, name, goal_level, parent_goal_id)
             VALUES (?1, '未设置最终目标', 'final', NULL)",
            params![profile_id],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get(id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn final_of(&self, profile_id: i64) -> rusqlite::Result<Option<Goal>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM goals WHERE profile_id = ?1 AND goal_level = 'final' LIMIT 1",
            GOAL_COLS
        ))?;
        let mut rows = stmt.query_map(params![profile_id], parse_goal)?;
        rows.next().transpose()
    }

    /// 创建目标树节点（§19/§22-24 全部规则在 Repository 层强校验）。
    ///
    /// - level=final：parent 必须 NULL；同 Profile 已有 final → 拒绝
    /// - level=year：parent 必须 final（同 Profile）；period = {year}-01-01 ~ {year}-12-31；同年唯一
    /// - level=month：parent 必须 year；{ym}-01-01 ~ 月末；月份年份必须等于 parent 年；同 parent 同月唯一
    /// - level=day：parent 必须 month；period = 当天；日期必须落在 parent 月；同 parent 同日唯一
    /// period 格式：year="2026" / month="2026-08" / day="2026-08-16"；final 忽略
    #[allow(clippy::too_many_arguments)]
    pub fn create_tree_node(
        &self,
        profile_id: i64,
        level: &str,
        parent_goal_id: Option<i64>,
        name: &str,
        description: Option<&str>,
        period: Option<&str>,
    ) -> Result<Goal, String> {
        let (ps, pe, parent): (Option<String>, Option<String>, Option<Goal>) = match level {
            "final" => {
                if parent_goal_id.is_some() {
                    return Err("最终目标不能有父目标".to_string());
                }
                if self.final_of(profile_id).map_err(|e| e.to_string())?.is_some() {
                    return Err("该档案已有最终目标（每档案只能有一个）".to_string());
                }
                (None, None, None)
            }
            "year" => {
                let parent = Self::expect_parent(self, parent_goal_id, "year", "final", profile_id)?;
                // DEV-0057 §32-36：Year Goal = 长周期规划阶段，允许跨自然年。
                // 双格式统一（与 AI/ChangeSet 链一致）："YYYY-MM-DD..YYYY-MM-DD" 区间 或 "YYYY"（自然年=区间特例）。
                let p = period.ok_or_else(|| {
                    "年目标需要周期，如 2026 或 2026-08-01..2027-08-01".to_string()
                })?;
                let (start, end) = if let Some(idx) = p.find("..") {
                    let (s, e) = (&p[..idx], &p[idx + 2..]);
                    let ok = |x: &str| x.len() == 10 && x.as_bytes()[4] == b'-' && x.as_bytes()[7] == b'-';
                    if !ok(s) || !ok(e) || s >= e {
                        return Err(format!("年目标周期区间非法：{p}"));
                    }
                    (s.to_string(), e.to_string())
                } else {
                    let y: i64 = p.parse().map_err(|_| format!("年目标周期非法：{p}"))?;
                    if !(1900..=2999).contains(&y) {
                        return Err("年份非法".to_string());
                    }
                    (format!("{y}-01-01"), format!("{y}-12-31"))
                };
                (Some(start), Some(end), Some(parent))
            }
            "month" => {
                let parent = Self::expect_parent(self, parent_goal_id, "month", "year", profile_id)?;
                let ym = period
                    .filter(|s| s.len() == 7 && s.as_bytes()[4] == b'-')
                    .ok_or_else(|| "月目标需要月份，如 2026-08".to_string())?;
                let (y, m): (i64, i64) = {
                    let mut it = ym.split('-');
                    let y = it.next().and_then(|v| v.parse().ok());
                    let m = it.next().and_then(|v| v.parse().ok());
                    match (y, m) {
                        (Some(y), Some(m)) if (1..=12).contains(&m) => (y, m),
                        _ => return Err("月份非法".to_string()),
                    }
                };
                // 月份必须落在 parent Year 区间内（DEV-0057 §34：父年可跨自然年，两链统一为区间包含）
                let ms = format!("{y}-{m:02}-01");
                let dim = days_in_month(y, m);
                let me = format!("{y}-{m:02}-{dim:02}");
                match (parent.period_start.as_deref(), parent.period_end.as_deref()) {
                    (Some(ys), Some(ye)) => {
                        if ms.as_str() < ys || me.as_str() > ye {
                            return Err("月目标必须创建在其父年目标的周期范围内".to_string());
                        }
                    }
                    _ => return Err("父年目标缺少周期".to_string()),
                }
                (
                    Some(format!("{y}-{m:02}-01")),
                    Some(format!("{y}-{m:02}-{dim:02}")),
                    Some(parent),
                )
            }
            "day" => {
                let parent = Self::expect_parent(self, parent_goal_id, "day", "month", profile_id)?;
                let d = period
                    .filter(|s| s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-')
                    .ok_or_else(|| "日目标需要日期，如 2026-08-16".to_string())?;
                // 日期必须属于 parent Month（§24）
                let (ps_m, pe_m) = match (parent.period_start.as_deref(), parent.period_end.as_deref()) {
                    (Some(s), Some(e)) => (s, e),
                    _ => return Err("父月目标缺少周期".to_string()),
                };
                if d < ps_m || d > pe_m {
                    return Err("日目标日期必须属于其父月目标".to_string());
                }
                (Some(d.to_string()), Some(d.to_string()), Some(parent))
            }
            other => return Err(format!("不支持的目标层级：{other}")),
        };

        // 环/自引用在结构上不可能：父节点必须恰为上一层级且已存在（新节点尚无子）。
        let parent_id = parent.as_ref().map(|p| p.id);
        let n = self
            .conn
            .execute(
                "INSERT INTO goals (profile_id, parent_goal_id, goal_level, name, description, period_start, period_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![profile_id, parent_id, level, name, description, ps, pe],
            )
            .map_err(|e| {
                // 唯一索引冲突 → 人话
                if e.to_string().contains("idx_goals_sibling_period_unique") {
                    "同一父目标下该周期已存在，禁止重复创建".to_string()
                } else if e.to_string().contains("idx_goals_final_unique") {
                    "该档案已有最终目标".to_string()
                } else {
                    e.to_string()
                }
            })?;
        if n == 0 {
            return Err("创建失败".to_string());
        }
        let id = self.conn.last_insert_rowid();
        self.get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "创建失败".to_string())
    }

    /// 校验父节点：存在、层级正确、同 Profile；返回父 Goal。跨 Profile/层级错误拒绝。
    fn expect_parent(
        &self,
        parent_goal_id: Option<i64>,
        child_level: &str,
        want_parent_level: &str,
        profile_id: i64,
    ) -> Result<Goal, String> {
        let pid = parent_goal_id.ok_or_else(|| match child_level {
            "year" => "年目标必须创建在最终目标下".to_string(),
            "month" => "月目标必须创建在年目标下".to_string(),
            _ => "日目标必须创建在月目标下".to_string(),
        })?;
        let parent = self
            .get(pid)
            .map_err(|e| e.to_string())?
            .ok_or("父目标不存在")?;
        if parent.goal_level != want_parent_level {
            return Err(match child_level {
                "year" => "年目标的父节点必须是最终目标".to_string(),
                "month" => "月目标的父节点必须是年目标".to_string(),
                _ => "日目标的父节点必须是月目标".to_string(),
            });
        }
        if parent.profile_id != Some(profile_id) {
            return Err("禁止跨档案创建子目标".to_string());
        }
        Ok(parent)
    }

    /// 读取整棵树（final → year → month → day；legacy 单列）。
    pub fn tree(&self, profile_id: i64) -> Result<GoalTree, String> {
        let all = self
            .list_by_profile(profile_id)
            .map_err(|e| e.to_string())?;
        let final_goal = all
            .iter()
            .find(|g| g.goal_level == "final")
            .cloned()
            .ok_or("该档案没有最终目标")?;
        Ok(GoalTree {
            final_goal: build_node(final_goal, &all),
            legacy_goals: all.iter().filter(|g| g.goal_level == "legacy").cloned().collect(),
        })
    }

    /// 删除目标（§27）：final 禁删；有子目标禁删（不自动提升/级联）；Task 保留（goal_id 由 FK SET NULL 置空）。
    pub fn delete_tree_node(&self, id: i64) -> Result<(), String> {
        let g = self.get(id).map_err(|e| e.to_string())?.ok_or("目标不存在")?;
        if g.goal_level == "final" {
            return Err("最终目标不能删除，只能编辑".to_string());
        }
        let child_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM goals WHERE parent_goal_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if child_count > 0 {
            return Err(match g.goal_level.as_str() {
                "year" => "该年目标仍包含月目标，请先处理其子目标。".to_string(),
                "month" => "该月目标仍包含日目标，请先处理其子目标。".to_string(),
                _ => "该目标仍包含子目标，请先处理其子目标。".to_string(),
            });
        }
        // Task.goal_id FK ON DELETE SET NULL → 自动保留任务
        self.conn
            .execute("DELETE FROM goals WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 某 Profile 的 legacy Stage/Plan 计数（§29 轻提示用）。
    pub fn legacy_planning_counts(&self, profile_id: i64) -> rusqlite::Result<(i64, i64)> {
        let stages: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM study_stages ss JOIN goals g ON ss.goal_id = g.id
             WHERE g.profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )?;
        let plans: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM plans p JOIN goals g ON p.goal_id = g.id
             WHERE g.profile_id = ?1",
            params![profile_id],
            |r| r.get(0),
        )?;
        Ok((stages, plans))
    }
}

fn build_node(g: Goal, all: &[Goal]) -> GoalTreeNode {
    let mut kids: Vec<Goal> = all
        .iter()
        .filter(|x| x.parent_goal_id == Some(g.id))
        .cloned()
        .collect();
    kids.sort_by(|a, b| a.period_start.cmp(&b.period_start).then(a.id.cmp(&b.id)));
    GoalTreeNode {
        children: kids.into_iter().map(|k| build_node(k, all)).collect(),
        goal: g,
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn parse_goal(row: &rusqlite::Row<'_>) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        profile_id: row.get(4)?,
        parent_goal_id: row.get(5)?,
        goal_level: row.get(6)?,
        period_start: row.get(7)?,
        period_end: row.get(8)?,
        sort_order: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        goal_brief_json: row.get(12)?,
    })
}
