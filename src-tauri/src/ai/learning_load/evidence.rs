//! DEV-0077.4-A · Evidence Builder（§六 / §十一-§十八 / §四十一-§五十五 / §九十五）。
//!
//! 唯一入口 [`build_learning_load_evidence`]：纯读取（§六），一次构建
//! LearningLoadEvidence 快照。
//!
//! 查询纪律（§四十一，禁 N+1）：固定 8 条批量 SELECT（learning_items /
//! tasks / study_sessions / evaluations / feedbacks / blueprint / mastery /
//! user_context），全部 Rust 内聚合；任何 Source 读取失败 → 整体 Err
//! （§九十五：禁止把失败当 0——0 会误读为「用户没学习」）。
//!
//! 关联纪律（§十二/§十三/§四十六/§四十七/§四十八）：
//! - Task/Session/Evaluation/Feedback 归属只用**外键**，禁止标题模糊匹配；
//! - Session effort 归属优先 Session snapshot learning_item_id（历史发生时的
//!   事实），snapshot 与 Task 不一致 → conflict 计数，不静默选边；
//! - pace sample 归 Task.learning_item_id（estimate 是 Task 的事实）。

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{params, Connection};

use super::pace::{build_pace_evidence, PaceSample};
use super::quality::{assess_profile_quality, assess_unit_quality};
use super::types::*;

/// §四十一：固定查询数（性能门报告口径；全部为批量 SELECT）。
pub const QUERY_COUNT: usize = 8;
/// §十五：Session duration 异常阈值（>18h → 质量降级信号，不删除事实）。
pub const ABNORMAL_SESSION_SECONDS: i64 = 18 * 3600;
/// §四十二：Pace 校准窗口（天）。
pub const PACE_WINDOW_DAYS: i64 = 90;
/// §四十二：Evaluation/Feedback 近期读取窗口（天）。
pub const RECENT_EVIDENCE_DAYS: i64 = 90;
/// §三十：Feedback recent_items 上限。
pub const FEEDBACK_RECENT_LIMIT: usize = 10;
/// Unit evidence_refs 上限（防 Context 膨胀）。
pub const UNIT_REF_LIMIT: usize = 20;

/// 简易公历日期偏移（YYYY-MM-DD；与既有测试 fixture 同族算法，零依赖）。
fn date_offset(base: &str, days: i64) -> String {
    let p: Vec<i64> = base.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() < 3 {
        return base.to_string();
    }
    let (mut y, mut m, mut d) = (p[0], p[1], p[2]);
    let dim = |yy: i64, mm: i64| -> i64 {
        match mm {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            _ => {
                if (yy % 4 == 0 && yy % 100 != 0) || yy % 400 == 0 {
                    29
                } else {
                    28
                }
            }
        }
    };
    let mut remaining = days;
    while remaining != 0 {
        if remaining > 0 {
            d += 1;
            if d > dim(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
            remaining -= 1;
        } else {
            d -= 1;
            if d < 1 {
                m -= 1;
                if m < 1 {
                    m = 12;
                    y -= 1;
                }
                d = dim(y, m);
            }
            remaining += 1;
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

/// §六：公开入口——纯读取构建 Evidence（不写任何表）。
pub fn build_learning_load_evidence(
    conn: &Connection,
    profile_id: i64,
    today: &str,
) -> Result<LearningLoadEvidence, String> {
    let cutoff_pace = date_offset(today, -PACE_WINDOW_DAYS);
    let cutoff_recent = date_offset(today, -RECENT_EVIDENCE_DAYS);
    let cutoff_30 = date_offset(today, -30);
    let cutoff_14 = date_offset(today, -14);
    let cutoff_7 = date_offset(today, -7);

    // ---- ① LearningItem Tree（§四十二：当前全部 active tree 节点）----
    struct ItemRow {
        id: i64,
        parent_id: Option<i64>,
        goal_id: Option<i64>,
        name: String,
        mastery_status: String,
    }
    let items: Vec<ItemRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_id, goal_id, name, mastery_status
                 FROM learning_items WHERE profile_id = ?1 ORDER BY id",
            )
            .map_err(|e| format!("learning_items 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                Ok(ItemRow {
                    id: r.get(0)?,
                    parent_id: r.get(1)?,
                    goal_id: r.get(2)?,
                    name: r.get(3)?,
                    mastery_status: r.get(4)?,
                })
            })
            .map_err(|e| format!("learning_items 读取失败: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("learning_items 读取失败: {e}"))?
    };
    let item_ids: BTreeSet<i64> = items.iter().map(|i| i.id).collect();
    let item_by_id: HashMap<i64, &ItemRow> = items.iter().map(|i| (i.id, i)).collect();

    // ---- ② Tasks（§十一/§四十四/§五十五：最近 90 天 + future + 历史完成参与 pace；
    //         全量单查后在 Rust 内分窗——2000 行量级无压力且避免边界遗漏）----
    struct TaskRow {
        id: i64,
        learning_item_id: Option<i64>,
        estimated_minutes: Option<i64>,
        status: String,
        planned_date: Option<String>,
        title: String,
        archived: bool,
    }
    let tasks: Vec<TaskRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, learning_item_id, estimated_minutes, status,
                        COALESCE(planned_date, ''), title, archived_at IS NOT NULL
                 FROM tasks WHERE profile_id = ?1 ORDER BY id",
            )
            .map_err(|e| format!("tasks 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                Ok(TaskRow {
                    id: r.get(0)?,
                    learning_item_id: r.get(1)?,
                    estimated_minutes: r.get(2)?,
                    status: r.get(3)?,
                    planned_date: {
                        let d: String = r.get(4)?;
                        if d.is_empty() { None } else { Some(d) }
                    },
                    title: r.get(5)?,
                    archived: r.get(6)?,
                })
            })
            .map_err(|e| format!("tasks 读取失败: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("tasks 读取失败: {e}"))?
    };

    // ---- ③ StudySessions（§十三/§十四：仅 completed & duration>0 计 actual；
    //         §十五：异常 duration 保留但降质量）----
    struct SessionRow {
        id: i64,
        task_id: Option<i64>,
        learning_item_id: Option<i64>,
        duration_seconds: i64,
        /// local study day（UTC+8 口径，与 mastery 系统一致）。
        day: String,
        ended_at: Option<String>,
        title: String,
    }
    let sessions: Vec<SessionRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, task_id, learning_item_id,
                        COALESCE(duration_seconds, 0), started_at, ended_at, title
                 FROM study_sessions
                 WHERE profile_id = ?1 AND status = 'completed'
                 ORDER BY started_at",
            )
            .map_err(|e| format!("study_sessions 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                let started: String = r.get(4)?;
                Ok(SessionRow {
                    id: r.get(0)?,
                    task_id: r.get(1)?,
                    learning_item_id: r.get(2)?,
                    duration_seconds: r.get(3)?,
                    day: day_of(&started),
                    ended_at: r.get(5)?,
                    title: r.get(6)?,
                })
            })
            .map_err(|e| format!("study_sessions 读取失败: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("study_sessions 读取失败: {e}"))?
    };

    // ---- ④ Evaluations（§二十六：按 learning_item_id 聚合；近期窗口）----
    struct EvalRow {
        id: i64,
        learning_item_id: Option<i64>,
        outcome: String,
        score: Option<f64>,
        max_score: Option<f64>,
        occurred_at: String,
        title: String,
    }
    let evals: Vec<EvalRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, learning_item_id, outcome, score, max_score,
                        COALESCE(occurred_at, ''), title
                 FROM evaluations
                 WHERE profile_id = ?1 AND date(COALESCE(occurred_at, '1970-01-01'), '+8 hours') >= ?2
                 ORDER BY occurred_at",
            )
            .map_err(|e| format!("evaluations 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id, cutoff_recent], |r| {
                Ok(EvalRow {
                    id: r.get(0)?,
                    learning_item_id: r.get(1)?,
                    outcome: r.get(2)?,
                    score: r.get(3)?,
                    max_score: r.get(4)?,
                    occurred_at: r.get(5)?,
                    title: r.get(6)?,
                })
            })
            .map_err(|e| format!("evaluations 读取失败: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("evaluations 读取失败: {e}"))?
    };

    // ---- ⑤ Feedbacks（§二十九：直连 learning_item_id 或 evaluation→item；
    //         无 profile_id → 经 goal 归属链；近期窗口）----
    struct FeedbackRow {
        id: i64,
        learning_item_id: Option<i64>,
        evaluation_id: Option<i64>,
        feedback_type: String,
        title: String,
        status: String,
        created_at: String,
    }
    let feedbacks: Vec<FeedbackRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT f.id, f.learning_item_id, f.evaluation_id, f.feedback_type,
                        f.title, f.status, COALESCE(f.created_at, '')
                 FROM feedbacks f
                 JOIN goals g ON f.goal_id = g.id
                 WHERE g.profile_id = ?1
                   AND date(COALESCE(f.created_at, '1970-01-01'), '+8 hours') >= ?2
                 ORDER BY f.created_at",
            )
            .map_err(|e| format!("feedbacks 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id, cutoff_recent], |r| {
                Ok(FeedbackRow {
                    id: r.get(0)?,
                    learning_item_id: r.get(1)?,
                    evaluation_id: r.get(2)?,
                    feedback_type: r.get(3)?,
                    title: r.get(4)?,
                    status: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })
            .map_err(|e| format!("feedbacks 读取失败: {e}"))?;
        rows.collect::<Result<_, _>>().map_err(|e| format!("feedbacks 读取失败: {e}"))?
    };
    // evaluation_id → learning_item_id 推导表（确定性关系推导，非文本猜测）。
    let eval_item: HashMap<i64, i64> = evals
        .iter()
        .filter_map(|e| e.learning_item_id.map(|i| (e.id, i)))
        .collect();

    // ---- ⑥ stated capacity（§三十四：最近 Blueprint 的规划假设，用户确认值）----
    // 无 Blueprint 行 → None（optional source；真正的读取错误仍按 §九十五 整体 Err）。
    let stated_daily_minutes: Option<i64> = {
        let raw: Option<String> = conn
            .query_row(
                "SELECT structured_json FROM planning_blueprints
                 WHERE profile_id = ?1 AND structured_json IS NOT NULL
                 ORDER BY id DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("planning_blueprints 读取失败: {e}"))?;
        raw.and_then(|j| {
            serde_json::from_str::<serde_json::Value>(&j)
                .ok()
                .and_then(|v| v.get("daily_available_minutes").and_then(|x| x.as_i64()))
                .filter(|m| *m > 0)
        })
    };

    // ---- ⑦ Mastery 可用性（§三十二/§九十六：goal/period 级最新 assessment）----
    let mastery = {
        let row: Option<(Option<i64>, String, String, String)> = conn
            .query_row(
                "SELECT score, confidence, period_type, COALESCE(period_start, '')
                 FROM mastery_assessments
                 WHERE profile_id = ?1 AND status = 'scored'
                 ORDER BY id DESC LIMIT 1",
                params![profile_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(|e| format!("mastery_assessments 读取失败: {e}"))?;
        MasteryEvidence {
            available: true,
            latest_score: row.as_ref().and_then(|r| r.0),
            latest_confidence: row.as_ref().map(|r| r.1.clone()),
            latest_period: row.as_ref().map(|r| {
                format!("{}@{}", r.2, r.3)
            }),
            note: String::from(
                "Unit 级 mastery 读 learning_items.mastery_status（真实值）；\
                 mastery_assessments 为 goal/period 级 AI 评估证据，不回写 mastery。",
            ),
        }
    };

    // ---- ⑧ UserContext（§三十四：learning-relevant 摘录，不整体复制）----
    let user_context_note: Option<String> = conn
        .query_row(
            "SELECT user_context_json FROM personalization_profiles WHERE profile_id = ?1",
            params![profile_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    let _ = user_context_note; // A 阶段仅确认可读；不进入 Evidence 结构（§三十四）

    // ================= Rust 内聚合 =================

    let mut conflicts = EvidenceConflictSummary::default();
    let mut summary = LearningEvidenceSummary {
        learning_item_count: items.len(),
        ..Default::default()
    };

    // Task per-item 聚合（§十一：learning_item_id != NULL 才进 Unit）
    struct TaskAgg {
        count: i64,
        completed: i64,
        pending: i64,
        planned_minutes: i64,
        titles_completed_no_session: Vec<String>,
    }
    let mut task_agg: BTreeMap<i64, TaskAgg> = BTreeMap::new();
    let mut task_by_id: HashMap<i64, &TaskRow> = HashMap::new();
    for t in &tasks {
        task_by_id.insert(t.id, t);
        match t.learning_item_id {
            Some(item) => {
                if !item_ids.contains(&item) {
                    // §五十五：安全删除的 LearningItem 不会保留关联（FK CASCADE）；
                    // 出现即孤儿引用 → 冲突计数，不猜归入。
                    conflicts.missing_learning_items += 1;
                    summary.unlinked_task_count += 1;
                    continue;
                }
                summary.linked_task_count += 1;
                let a = task_agg.entry(item).or_insert_with(|| TaskAgg {
                    count: 0,
                    completed: 0,
                    pending: 0,
                    planned_minutes: 0,
                    titles_completed_no_session: Vec::new(),
                });
                a.count += 1;
                match t.status.as_str() {
                    "completed" => a.completed += 1,
                    "pending" | "in_progress" => a.pending += 1,
                    _ => {}
                }
                if let Some(m) = t.estimated_minutes {
                    if !(super::pace::ESTIMATE_MIN..=super::pace::ESTIMATE_MAX).contains(&m) {
                        conflicts.invalid_task_estimates += 1;
                    } else if !t.archived || t.status == "completed" {
                        // §五十五：archived completed 仍参与历史 planned 统计
                        a.planned_minutes += m;
                    }
                }
            }
            None => {
                // §十二：learning_item_id NULL → unassigned，禁止标题猜关联
                summary.unlinked_task_count += 1;
            }
        }
    }

    // Session per-item actual（§十三归属 / §十四时长 / §十五异常 / §十七求和）
    struct SessionAgg {
        count: i64,
        minutes: i64,
        sample_titles: Vec<String>,
    }
    let mut session_agg: BTreeMap<i64, SessionAgg> = BTreeMap::new();
    // task_id → (minutes, abnormal) 供 pace sample
    let mut task_actual: HashMap<i64, (i64, bool)> = HashMap::new();
    let mut day_minutes: BTreeMap<String, i64> = BTreeMap::new();
    for s in &sessions {
        if s.duration_seconds <= 0 {
            // §十四：非法 duration 不计 actual（事实仍在 DB，不修改）
            conflicts.invalid_session_durations += 1;
            continue;
        }
        let abnormal = s.duration_seconds > ABNORMAL_SESSION_SECONDS;
        if abnormal {
            conflicts.invalid_session_durations += 1;
        }
        let minutes = (s.duration_seconds as f64 / 60.0).round() as i64; // §十四：round 全局统一
        day_minutes
            .entry(s.day.clone())
            .and_modify(|m| *m += minutes)
            .or_insert(minutes);

        // Task 维度 actual（不管 session 是否有 item 归属）
        if let Some(tid) = s.task_id {
            let e = task_actual.entry(tid).or_insert((0, false));
            e.0 += minutes;
            e.1 |= abnormal;
        }

        // Unit 归属（§四十七/§四十八：snapshot 优先）
        let owner: Option<i64> = if let Some(item) = s.learning_item_id {
            if !item_ids.contains(&item) {
                conflicts.missing_learning_items += 1;
                None
            } else {
                // 冲突检测：snapshot 与 task 链不一致 → 计数，仍归 snapshot
                if let Some(tid) = s.task_id {
                    if let Some(t) = task_by_id.get(&tid) {
                        if let Some(task_item) = t.learning_item_id {
                            if task_item != item {
                                conflicts.session_task_learning_item_conflicts += 1;
                            }
                        }
                    }
                }
                Some(item)
            }
        } else if let Some(tid) = s.task_id {
            // §十三：snapshot NULL 但 task 链存在 → 确定性关系推导
            task_by_id
                .get(&tid)
                .and_then(|t| t.learning_item_id)
                .filter(|i| item_ids.contains(i))
        } else {
            None
        };

        match owner {
            Some(item) => {
                summary.linked_session_count += 1;
                let a = session_agg.entry(item).or_insert_with(|| SessionAgg {
                    count: 0,
                    minutes: 0,
                    sample_titles: Vec::new(),
                });
                a.count += 1;
                a.minutes += minutes;
                if a.sample_titles.len() < 4 {
                    a.sample_titles.push(s.title.clone());
                }
            }
            None => summary.unlinked_session_count += 1,
        }
    }

    // Pace samples（§五十六-§五十八：estimate↔actual 成对；completed 优先）
    let mut unit_pace_samples: BTreeMap<i64, Vec<PaceSample>> = BTreeMap::new();
    let mut all_pace_samples: Vec<PaceSample> = Vec::new();
    let mut session_without_estimate = 0i64;
    for t in &tasks {
        let Some(actual) = task_actual.get(&t.id) else { continue };
        match t.estimated_minutes {
            None => {
                // §五十八：actual 有了但 Task 无 estimate → 不进 calibration
                session_without_estimate += 1;
                continue;
            }
            Some(est) => {
                if !(super::pace::ESTIMATE_MIN..=super::pace::ESTIMATE_MAX).contains(&est) {
                    session_without_estimate += 1;
                    continue;
                }
                // §五十六第一版：pending Task 即使有 Session 也不作校准样本
                if t.status != "completed" {
                    continue;
                }
                if let Some(sample) = PaceSample::new(
                    t.id,
                    t.learning_item_id,
                    est,
                    actual.0,
                    actual.1,
                ) {
                    if let Some(item) = t.learning_item_id {
                        if item_ids.contains(&item) {
                            unit_pace_samples.entry(item).or_default().push(sample.clone());
                        }
                    }
                    all_pace_samples.push(sample);
                }
            }
        }
    }
    summary.session_without_estimate_count = session_without_estimate;

    // completed 无 Session（§五十七）
    for t in &tasks {
        if t.status == "completed"
            && !task_actual.contains_key(&t.id)
            && t.learning_item_id.is_some()
        {
            summary.completed_without_session_count += 1;
        }
    }

    // Evaluation per-item（§二十六/§二十八）
    #[derive(Default)]
    struct EvalAgg {
        rows: Vec<(i64, String, Option<f64>, Option<f64>, String, String)>, // id,outcome,score,max,occurred,title
    }
    let mut eval_agg: BTreeMap<i64, EvalAgg> = BTreeMap::new();
    for e in &evals {
        let owner = match e.learning_item_id {
            Some(i) if item_ids.contains(&i) => Some(i),
            _ => None,
        };
        if let Some(i) = owner {
            eval_agg.entry(i).or_default().rows.push((
                e.id,
                e.outcome.clone(),
                e.score,
                e.max_score,
                e.occurred_at.clone(),
                e.title.clone(),
            ));
        }
        if e.learning_item_id.is_some() {
            summary.evaluation_count += 1;
        }
    }

    // Feedback per-item（§二十九：直连或 evaluation 推导）
    let mut fb_agg: BTreeMap<i64, Vec<(i64, String, String, String, String)>> = BTreeMap::new();
    for f in &feedbacks {
        summary.feedback_count += 1;
        let owner = f.learning_item_id.filter(|i| item_ids.contains(i)).or_else(|| {
            f.evaluation_id.and_then(|eid| eval_item.get(&eid).copied().filter(|i| item_ids.contains(i)))
        });
        if let Some(i) = owner {
            fb_agg.entry(i).or_default().push((
                f.id,
                f.feedback_type.clone(),
                f.title.clone(),
                f.status.clone(),
                f.created_at.clone(),
            ));
        }
    }

    // Subject 归属（§二十四：LearningItem Tree 顶层 root；防环）
    fn subject_root<'a>(
        start: i64,
        by_id: &HashMap<i64, &'a ItemRow>,
    ) -> (Option<i64>, &'a str) {
        let mut cur = start;
        let mut visited = BTreeSet::new();
        let mut name: &str = "unknown";
        let mut root: Option<i64> = None;
        loop {
            if !visited.insert(cur) {
                return (None, "unknown"); // 环 → unknown
            }
            match by_id.get(&cur) {
                Some(item) => {
                    name = &item.name;
                    match item.parent_id {
                        Some(p) if by_id.contains_key(&p) => cur = p,
                        _ => {
                            root = Some(cur);
                            return (root, name);
                        }
                    }
                }
                None => return (None, "unknown"),
            }
        }
    }
    let mut item_subject: HashMap<i64, (Option<i64>, String)> = HashMap::new();
    for it in &items {
        item_subject.insert(it.id, {
            let (sid, name) = subject_root(it.id, &item_by_id);
            (sid, name.to_string())
        });
    }

    // ---- Unit Evidence 组装（§九）----
    let mut units: Vec<LearningUnitEvidence> = Vec::new();
    for it in &items {
        let ta = task_agg.get(&it.id);
        let sa = session_agg.get(&it.id);
        let ea = eval_agg.get(&it.id);
        let fa = fb_agg.get(&it.id);
        let pace_samples = unit_pace_samples.get(&it.id).cloned().unwrap_or_default();
        let pace = build_pace_evidence(&pace_samples);

        // Evaluation 摘要（§二十六/§二十八：Evidence ≠ Mastery）
        let mut evaluation = EvaluationEvidence::default();
        if let Some(agg) = ea {
            evaluation.count = agg.rows.len() as i64;
            // rows 按查询 ORDER BY occurred_at 升序——直接取末尾为 latest
            for (_, outcome, score, max_score, _, _) in &agg.rows {
                match outcome.as_str() {
                    "passed" => evaluation.passed_count += 1,
                    "partial" => evaluation.partial_count += 1,
                    "failed" => evaluation.failed_count += 1,
                    _ => {}
                }
                if outcome != "unrated" {
                    evaluation.rated_count += 1;
                }
                // §二十八：最近有效 score ratio（升序扫描，后到覆盖）
                if let (Some(s), Some(m)) = (score, max_score) {
                    if *m > 0.0 {
                        evaluation.recent_score_ratio = Some((*s / *m).clamp(0.0, 1.0));
                    }
                }
            }
            if let Some((_, outcome, _, _, occurred, _)) = agg.rows.last() {
                evaluation.latest_outcome = Some(outcome.clone());
                evaluation.latest_at = Some(occurred.clone());
            }
        }

        // Feedback 摘要（§三十/§三十一：不做语义合并）
        let mut feedback = FeedbackEvidenceSummary::default();
        if let Some(rows) = fa {
            feedback.count = rows.len() as i64;
            let mut sorted = rows.clone();
            sorted.sort_by(|a, b| b.4.cmp(&a.4)); // created_at DESC
            for (id, ftype, title, status, created) in sorted {
                match ftype.as_str() {
                    "weakness" => feedback.weakness_count += 1,
                    "error" => feedback.error_count += 1,
                    "blocker" => feedback.blocker_count += 1,
                    "observation" => feedback.observation_count += 1,
                    _ => {}
                }
                if feedback.recent_items.len() < FEEDBACK_RECENT_LIMIT {
                    feedback.recent_items.push(FeedbackEvidenceItem {
                        id,
                        feedback_type: ftype,
                        title,
                        status,
                        created_at: created,
                    });
                }
            }
        }

        let session_count = sa.map(|s| s.count).unwrap_or(0);
        let actual_minutes = sa.map(|s| s.minutes).unwrap_or(0);
        let quality = assess_unit_quality(&pace, &evaluation, session_count, &feedback);

        // 溯源 refs（§十：代表性事实，上限 20）
        let mut refs: Vec<EvidenceRef> = Vec::new();
        if let Some(ta) = ta {
            if ta.completed > 0 {
                refs.push(EvidenceRef {
                    source_type: EvidenceSourceType::Task,
                    entity_id: it.id,
                    occurred_at: None,
                    summary: format!(
                        "Task {}（completed {} / pending {}，planned {} min）",
                        ta.count, ta.completed, ta.pending, ta.planned_minutes
                    ),
                });
            }
        }
        if let Some(sa) = sa {
            refs.push(EvidenceRef {
                source_type: EvidenceSourceType::StudySession,
                entity_id: it.id,
                occurred_at: None,
                summary: format!("Session {}（actual {} min）", sa.count, sa.minutes),
            });
        }
        if let Some(agg) = ea {
            if let Some((id, outcome, _, _, occurred, _)) = agg.rows.last() {
                refs.push(EvidenceRef {
                    source_type: EvidenceSourceType::Evaluation,
                    entity_id: *id,
                    occurred_at: Some(occurred.clone()),
                    summary: format!("latest outcome = {outcome}"),
                });
            }
        }
        if let Some(rows) = fa {
            if let Some((id, ftype, title, _, created)) = rows.first() {
                refs.push(EvidenceRef {
                    source_type: EvidenceSourceType::Feedback,
                    entity_id: *id,
                    occurred_at: Some(created.clone()),
                    summary: format!("{ftype}: {title}"),
                });
            }
        }
        refs.truncate(UNIT_REF_LIMIT);

        units.push(LearningUnitEvidence {
            learning_item_id: it.id,
            name: it.name.clone(),
            parent_id: it.parent_id,
            goal_id: it.goal_id,
            mastery_status: it.mastery_status.clone(),
            task_count: ta.map(|t| t.count).unwrap_or(0),
            completed_task_count: ta.map(|t| t.completed).unwrap_or(0),
            pending_task_count: ta.map(|t| t.pending).unwrap_or(0),
            planned_minutes: ta.map(|t| t.planned_minutes).unwrap_or(0),
            session_count,
            actual_minutes,
            pace,
            evaluation,
            feedback,
            evidence_quality: quality,
            evidence_refs: refs,
        });
    }

    // ---- Subject / Global Pace（§二十三/§二十四）----
    let mut subject_buckets: BTreeMap<(Option<i64>, String), Vec<PaceSample>> = BTreeMap::new();
    for (item, samples) in &unit_pace_samples {
        if let Some((sid, sname)) = item_subject.get(item) {
            subject_buckets
                .entry((*sid, sname.to_string()))
                .or_default()
                .extend(samples.iter().cloned());
        }
    }
    let mut subject_pace: Vec<SubjectPaceEvidence> = subject_buckets
        .into_iter()
        .map(|((sid, sname), samples)| SubjectPaceEvidence {
            subject_item_id: sid,
            subject_name: sname,
            unit_count: unit_pace_samples
                .keys()
                .filter(|i| item_subject.get(i).map_or(false, |(s, _)| *s == sid))
                .count(),
            pace: build_pace_evidence(&samples),
        })
        .collect();
    subject_pace.sort_by(|a, b| b.pace.sample_count.cmp(&a.pace.sample_count));
    let global_pace = build_pace_evidence(&all_pace_samples);
    summary.pace_sample_count = all_pace_samples.len();

    // ---- Observed Capacity（§三十五/§三十六：按 local study day 聚合）----
    let observed_for = |days: i64, cutoff: &str| -> Option<i64> {
        let total: i64 = day_minutes
            .iter()
            .filter(|(d, _)| d.as_str() >= cutoff)
            .map(|(_, m)| m)
            .sum();
        // 无任何学习日 → None（区别于「观测到 0 分钟」）；有学习记录才算观测值
        let active_days = day_minutes.keys().filter(|d| d.as_str() >= cutoff).count();
        if active_days == 0 {
            None
        } else {
            Some((total as f64 / days as f64).round() as i64)
        }
    };
    let total_30: i64 = day_minutes
        .iter()
        .filter(|(d, _)| d.as_str() >= cutoff_30.as_str())
        .map(|(_, m)| m)
        .sum();
    summary.observed_study_minutes_30d = total_30;
    let active_days_30 = day_minutes
        .keys()
        .filter(|d| d.as_str() >= cutoff_30.as_str())
        .count() as i64;
    let capacity = CapacityEvidence {
        stated_daily_minutes,
        observed_daily_minutes_7d: observed_for(7, &cutoff_7),
        observed_daily_minutes_14d: observed_for(14, &cutoff_14),
        observed_daily_minutes_30d: observed_for(30, &cutoff_30),
        active_study_days_30d: active_days_30,
        active_day_average_minutes_30d: if active_days_30 > 0 {
            Some((total_30 as f64 / active_days_30 as f64).round() as i64)
        } else {
            None
        },
    };

    let evidence_quality = assess_profile_quality(&units, &summary, &conflicts);

    Ok(LearningLoadEvidence {
        profile_id,
        generated_at: format!("{today}T00:00:00+08:00"),
        windows: EvidenceWindows::default(),
        units,
        subject_pace,
        global_pace,
        capacity,
        data_summary: summary,
        conflicts,
        mastery,
        evidence_quality,
    })
}

/// local study day（UTC+8 口径；started_at 形如 `YYYY-MM-DD HH:MM:SS`）。
fn day_of(started_at: &str) -> String {
    started_at
        .split(|c| c == ' ' || c == 'T')
        .next()
        .unwrap_or(started_at)
        .to_string()
}

/// §五十：debug-only pretty formatter（开发检查用，不做正式 UI）。
pub fn format_learning_load_evidence(ev: &LearningLoadEvidence) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "== Learning Load Evidence（profile {} @ {}）==\n",
        ev.profile_id, ev.generated_at
    ));
    let s = &ev.data_summary;
    out.push_str(&format!(
        "Summary: items={} linked_tasks={} unlinked_tasks={} linked_sessions={} unlinked_sessions={} evals={} feedback={} pace_samples={} minutes_30d={}\n",
        s.learning_item_count,
        s.linked_task_count,
        s.unlinked_task_count,
        s.linked_session_count,
        s.unlinked_session_count,
        s.evaluation_count,
        s.feedback_count,
        s.pace_sample_count,
        s.observed_study_minutes_30d
    ));
    let c = &ev.conflicts;
    out.push_str(&format!(
        "Conflicts: session_task_item={} missing_items={} bad_estimates={} bad_durations={}\n",
        c.session_task_learning_item_conflicts,
        c.missing_learning_items,
        c.invalid_task_estimates,
        c.invalid_session_durations
    ));
    let cap = &ev.capacity;
    out.push_str(&format!(
        "Capacity: stated={}min/day observed(7/14/30)={:?}/{:?}/{:?}min active_days_30d={} active_day_avg={:?}min\n",
        cap.stated_daily_minutes.unwrap_or(0),
        cap.observed_daily_minutes_7d,
        cap.observed_daily_minutes_14d,
        cap.observed_daily_minutes_30d,
        cap.active_study_days_30d,
        cap.active_day_average_minutes_30d
    ));
    out.push_str(&format!(
        "Global Pace: samples={} median={:?} calibrated={:.2}x confidence={}\n",
        ev.global_pace.sample_count,
        ev.global_pace.median_ratio,
        ev.global_pace.calibrated_ratio,
        ev.global_pace.confidence.as_str()
    ));
    for sp in ev.subject_pace.iter().take(5) {
        out.push_str(&format!(
            "  Subject [{}]: units={} samples={} calibrated={:.2}x\n",
            sp.subject_name, sp.unit_count, sp.pace.sample_count, sp.pace.calibrated_ratio
        ));
    }
    out.push_str("Units:\n");
    for u in &ev.units {
        out.push_str(&format!(
            "  - {} (mastery={}): tasks={} (completed {}/pending {}) planned={}min | sessions={} actual={}min | pace {:.2}x samples={} conf={} out={} | eval {}p/{}pa/{}f | fb w{} e{} b{} o{} | quality={}\n",
            u.name,
            u.mastery_status,
            u.task_count,
            u.completed_task_count,
            u.pending_task_count,
            u.planned_minutes,
            u.session_count,
            u.actual_minutes,
            u.pace.calibrated_ratio,
            u.pace.sample_count,
            u.pace.confidence.as_str(),
            u.pace.outlier_count,
            u.evaluation.passed_count,
            u.evaluation.partial_count,
            u.evaluation.failed_count,
            u.feedback.weakness_count,
            u.feedback.error_count,
            u.feedback.blocker_count,
            u.feedback.observation_count,
            u.evidence_quality.quality.as_str()
        ));
    }
    out
}

/// rusqlite OptionalExtension 便捷引入（read-only 查询用）。
trait OptionalRow<T> {
    fn optional(self) -> Result<Option<T>, String>;
}
impl<T> OptionalRow<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, String> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}
