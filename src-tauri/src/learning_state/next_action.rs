//! HIGHER CLOSED LOOP V1 — PHASE 2 / PHASE 3：Next Best Learning Action。
//!
//! **迁移**（不是第二套引擎）：本模块的类别优先级与 tie-break 规则逐条迁移自
//! `src/learning/startHere.ts`（PRODUCT-2.0 §0B.2 / §0C.5）。迁移完成后前端
//! `startHere.ts` 不再保留生产路径——同一时刻只有一个推荐引擎。
//!
//! 类别优先级（不可调换）：
//!   0 active session  → 硬规则：Primary 必须 Continue Active
//!   2 recovery        → PHASE 6 Recovery（短、容易开始）
//!   3 continue_last   → 最近未完成且连续性价值高
//!   4 review_due      → 阶段复盘 due（PHASE 7 链路入口）
//!   5 planned_task    → Today 明确高优先任务
//!   6 quick_study     → 兜底（手动学习始终可用）
//!
//! 同类别内 tie-break（迁移自 §0C.5，顺序不可调换）：
//!   1. hard deadline 更近 → 2. priority 更高 → 3. 与 available_minutes 更匹配
//!   → 4. 最近中断/未完成更近 → 5. 稳定 id
//!
//! 0 LLM：本模块不引用任何 provider / runtime / agent 符号。

use crate::learning_state::budget::TimeBudget;
use crate::learning_state::types::{
    ActionSource, ExecutionPayload, LearningStateSnapshot, MicroActionCandidate,
    NextActionAlternative, NextActionType, NextLearningAction, CONTINUE_LAST_WINDOW_DAYS,
    MAX_ALTERNATIVES, REASON_ACTIVE_SESSION, REASON_CONTINUE_LAST, REASON_MICRO_ACTION,
    REASON_MICRO_UNAVAILABLE, REASON_PLANNED_TASK, REASON_PLANNED_TASK_CORE,
    REASON_PLANNED_TASK_SLICE, REASON_QUICK_STUDY, REASON_RECOVERY, REASON_REVIEW_DUE,
};
use crate::repository::daily_report::DailyTaskRow;
use std::cmp::Ordering;

/// 兜底快速学习时长（迁移自 `startHere.ts` 的 `minutesFitOf(25, ...)`）。
pub const QUICK_STUDY_DEFAULT_MINUTES: i64 = 25;
/// PHASE 6：Recovery 默认动作时长（“2 分钟恢复动作”）。
pub const RECOVERY_DEFAULT_MINUTES: i64 = 2;
/// M0-A：30 秒档拿不到 grounded Micro 时，如实告知用户「Micro 不可用」的固定文案。
pub const MICRO_UNAVAILABLE_REASON: &str =
    "这一档需要一条有真实来源的小步，现在还没有。已换成 3 分钟的小步，仍然不会创建学习记录。";

// =============== 迁移的纯函数工具 ===============

/// SQLite datetime（UTC）→ ms（迁移自 `parseUtcMs`）；非法输入 → 0。
pub fn parse_utc_ms(raw: Option<&str>) -> i64 {
    let raw = match raw {
        Some(s) if !s.is_empty() => s,
        _ => return 0,
    };
    let normalized = if raw.contains('T') {
        raw.to_string()
    } else {
        format!("{}Z", raw.replace(' ', "T"))
    };
    chrono::DateTime::parse_from_rfc3339(&normalized)
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// "HH:MM" → 当天分钟数（迁移自 `parsePlannedMinutes`）；非法 → None。
pub fn parse_planned_minutes(planned_time: Option<&str>) -> Option<i64> {
    let s = planned_time?.trim();
    let mut it = s.split(':');
    let h: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    if !(0..=23).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    Some(h * 60 + m)
}

/// 迁移自 `priorityRankOf`：accumulation=2 / core=0 / 其他=1。
fn priority_rank_of(t: &DailyTaskRow) -> i32 {
    if t.task_kind == "accumulation" {
        2
    } else if t.priority == "core" {
        0
    } else {
        1
    }
}

/// 迁移自 `minutesFitOf`：|estimated - available|；无法比较 → MAX。
fn minutes_fit_of(estimated: Option<i64>, available: Option<i64>) -> i64 {
    match (estimated, available) {
        (Some(e), Some(a)) => (e - a).abs(),
        _ => i64::MAX,
    }
}

// =============== 候选 ===============

/// canonical 候选 Primitive（**唯一**一份；M1-A 的 Learning Pack 直接消费它，
/// 不做第二套推荐引擎、不重排）。
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub(crate) action_type: NextActionType,
    pub(crate) reason_code: String,
    pub(crate) source: ActionSource,
    pub(crate) title: String,
    pub(crate) subtitle: Option<String>,
    pub(crate) reasons: Vec<String>,
    pub(crate) payload_kind: String,
    pub(crate) task_id: Option<i64>,
    pub(crate) learning_item_id: Option<i64>,
    pub(crate) session_id: Option<i64>,
    pub(crate) review_id: Option<i64>,
    /// 动作的自然时长（任务估时 / 上次实际时长 / Recovery 默认 / 快速学习默认）。
    pub(crate) base_estimate: Option<i64>,
    /// 是否为任务级动作（entry_slice 只对任务级动作有意义）。
    pub(crate) task_scoped: bool,
    /// 是否天然允许被时间档裁剪（Recovery / quick_study）。
    pub(crate) slice_by_design: bool,

    // ---- tie-break keys（迁移自 §0C.5）----
    pub(crate) deadline_minutes: Option<i64>,
    pub(crate) priority_rank: i32,
    pub(crate) minutes_fit: i64,
    pub(crate) recency: i64,
    pub(crate) stable_id: i64,
}

/// §0C.5 tie-break #1～#6（迁移自 `compareStartHere`，不依赖任何外部状态）。
fn compare_candidates(a: &Candidate, b: &Candidate) -> Ordering {
    // 1. 类别优先级
    let cat = a.action_type.category_rank() - b.action_type.category_rank();
    if cat != 0 {
        return cat.cmp(&0);
    }
    // 2. hard deadline 更近：有时间的排在无时间前面
    match (a.deadline_minutes, b.deadline_minutes) {
        (Some(x), Some(y)) if x != y => return x.cmp(&y),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        _ => {}
    }
    // 3. priority 更高
    if a.priority_rank != b.priority_rank {
        return a.priority_rank.cmp(&b.priority_rank);
    }
    // 4. 与 available_minutes 更匹配
    if a.minutes_fit != b.minutes_fit {
        return a.minutes_fit.cmp(&b.minutes_fit);
    }
    // 5. 最近中断 / 未完成更近（recency 大者优先）
    if a.recency != b.recency {
        return b.recency.cmp(&a.recency);
    }
    // 6. 稳定排序
    a.stable_id.cmp(&b.stable_id)
}

/// PHASE 3：时间档裁剪。返回 (本次建议分钟, 是否只执行入口切片)。
///
/// 不变量：给定时间档时，返回值第一项的分钟数**恒 <= 时间档分钟数**
/// （30 秒档单列，见 `build_next_learning_action`）。
fn apply_budget(
    base: Option<i64>,
    budget: Option<TimeBudget>,
    task_scoped: bool,
) -> (Option<i64>, bool) {
    match (base, budget) {
        (Some(e), Some(bud)) => {
            let bm = bud.minutes();
            if e > bm {
                (Some(bm), task_scoped)
            } else {
                (Some(e), false)
            }
        }
        (Some(e), None) => (Some(e), false),
        // 估时未知的任务级动作：只承诺时间档内的入口切片，绝不声称完成任务
        (None, Some(bud)) if task_scoped => (Some(bud.minutes()), true),
        (None, _) => (None, false),
    }
}

/// 候选在时间档内是否可**完整**执行（slice_by_design 的短动作恒可）。
fn fits_budget(c: &Candidate, budget: TimeBudget) -> bool {
    if c.slice_by_design {
        return true;
    }
    match c.base_estimate {
        Some(e) => e <= budget.minutes(),
        None => false,
    }
}

// =============== 候选构造 ===============

fn active_session_candidate(snapshot: &LearningStateSnapshot) -> Option<Candidate> {
    let active = snapshot.active_session.as_ref()?;
    let minutes = active
        .duration_seconds
        .map(|s| (s / 60).max(0))
        .filter(|m| *m > 0);
    Some(Candidate {
        action_type: NextActionType::ActiveSession,
        reason_code: REASON_ACTIVE_SESSION.to_string(),
        source: ActionSource::Session {
            session_id: active.id,
        },
        title: if active.title.trim().is_empty() {
            "未命名学习".to_string()
        } else {
            active.title.clone()
        },
        subtitle: Some("已有一条进行中的学习记录".to_string()),
        reasons: vec![
            "当前已经有一条正在进行的学习记录。".to_string(),
            "继续已有的记录，不会新开第二条。".to_string(),
        ],
        payload_kind: "continue_session".to_string(),
        task_id: active.task_id,
        learning_item_id: active.learning_item_id,
        session_id: Some(active.id),
        review_id: None,
        base_estimate: minutes,
        task_scoped: false,
        slice_by_design: true,
        deadline_minutes: None,
        priority_rank: 0,
        minutes_fit: 0,
        recency: parse_utc_ms(Some(&active.started_at)),
        stable_id: active.id,
    })
}

/// PHASE 6：Recovery 的 Primary 优先「短、容易开始、与主目标相关」。
fn recovery_candidate(snapshot: &LearningStateSnapshot) -> Option<Candidate> {
    if !snapshot.recovery_state.should_take_primary {
        return None;
    }
    let signals = &snapshot.recovery_state.signals;

    // 选估时最小的未完成任务（与主目标相关优先：有 goal_id 的排前）
    let mut open: Vec<&DailyTaskRow> = snapshot
        .today_tasks
        .iter()
        .filter(|t| t.status != "completed")
        .collect();
    open.sort_by(|a, b| {
        let ag = a.goal_id.is_none() as i32;
        let bg = b.goal_id.is_none() as i32;
        ag.cmp(&bg)
            .then(
                a.estimated_minutes
                    .unwrap_or(RECOVERY_DEFAULT_MINUTES)
                    .cmp(&b.estimated_minutes.unwrap_or(RECOVERY_DEFAULT_MINUTES)),
            )
            .then(a.id.cmp(&b.id))
    });
    let picked = open.first().copied();

    let (title, subtitle, payload_kind, task_id, learning_item_id, base, task_scoped) = match picked
    {
        Some(t) => (
            t.title.clone(),
            Some(format!(
                "先做 {} 分钟就够",
                t.estimated_minutes
                    .unwrap_or(RECOVERY_DEFAULT_MINUTES)
                    .min(RECOVERY_DEFAULT_MINUTES)
            )),
            "start_task".to_string(),
            Some(t.id),
            t.learning_item_id,
            t.estimated_minutes,
            true,
        ),
        None => (
            format!("{} 分钟恢复动作", RECOVERY_DEFAULT_MINUTES),
            Some("从最小的一步开始".to_string()),
            "start_quick".to_string(),
            None,
            None,
            Some(RECOVERY_DEFAULT_MINUTES),
            false,
        ),
    };

    let mut reasons: Vec<String> = Vec::new();
    for code in &snapshot.recovery_state.reason_codes {
        reasons.push(match code.as_str() {
            crate::learning_state::types::RECOVERY_NO_RECENT_SESSIONS => {
                match signals.days_since_last_session {
                    Some(d) => format!("已连续 {} 天没有真实学习记录。", d),
                    None => "近期没有真实学习记录。".to_string(),
                }
            }
            crate::learning_state::types::RECOVERY_TASK_BACKLOG => {
                format!(
                    "近 7 天有 {} 条计划任务逾期未完成。",
                    signals.overdue_task_count_7d
                )
            }
            crate::learning_state::types::RECOVERY_COMPLETION_DROP => {
                match signals.completion_rate_7d {
                    Some(r) => format!("近 7 天完成率为 {:.0}%。", r * 100.0),
                    None => "近期完成率下降。".to_string(),
                }
            }
            crate::learning_state::types::RECOVERY_LOAD_OVER_CAPACITY => {
                "近期计划负荷明显高于你的实际可用时间。".to_string()
            }
            other => format!("触发条件：{}", other),
        });
    }
    reasons.push("这次只要求一个短动作，先把状态接回来。".to_string());

    Some(Candidate {
        action_type: NextActionType::Recovery,
        reason_code: format!(
            "{}_{}",
            REASON_RECOVERY,
            snapshot
                .recovery_state
                .reason_codes
                .first()
                .map(|s| s.as_str())
                .unwrap_or("signal")
        ),
        source: match task_id {
            Some(id) => ActionSource::Task { task_id: id },
            None => match learning_item_id {
                Some(i) => ActionSource::LearningItem {
                    learning_item_id: i,
                },
                None => ActionSource::None,
            },
        },
        title,
        subtitle,
        reasons,
        payload_kind,
        task_id,
        learning_item_id,
        session_id: None,
        review_id: None,
        base_estimate: base,
        task_scoped,
        // Recovery 刻意允许被时间档裁剪（短动作优先）
        slice_by_design: true,
        deadline_minutes: None,
        priority_rank: 0,
        minutes_fit: 0,
        recency: 0,
        stable_id: picked.map(|t| t.id).unwrap_or(0),
    })
}

fn review_due_candidate(snapshot: &LearningStateSnapshot) -> Option<Candidate> {
    if !snapshot.review_state.due && snapshot.review_state.open_review_id.is_none() {
        return None;
    }
    let rid = snapshot.review_state.open_review_id;
    Some(Candidate {
        action_type: NextActionType::ReviewDue,
        reason_code: REASON_REVIEW_DUE.to_string(),
        source: ActionSource::Review { review_id: rid },
        title: "该进行阶段复盘了".to_string(),
        subtitle: Some("读同一套学习证据，生成一次调整建议".to_string()),
        reasons: vec![
            format!(
                "当前周期复盘到期（周期 {} 天）。",
                snapshot.planning_state.review_interval_days.unwrap_or(14)
            ),
            "复盘只会产出**一个** ChangeSet，需要你确认后才会改动计划。".to_string(),
        ],
        payload_kind: "open_review".to_string(),
        task_id: None,
        learning_item_id: None,
        session_id: None,
        review_id: rid,
        base_estimate: None,
        task_scoped: false,
        slice_by_design: true,
        deadline_minutes: None,
        priority_rank: 0,
        minutes_fit: 0,
        recency: 0,
        stable_id: rid.unwrap_or(0),
    })
}

fn continue_last_candidate(
    snapshot: &LearningStateSnapshot,
    today: &str,
    available: Option<i64>,
) -> Option<Candidate> {
    let cutoff =
        crate::learning_state::date::date_offset(today, -CONTINUE_LAST_WINDOW_DAYS).ok()?;
    let last = snapshot
        .recent_sessions
        .iter()
        .filter(|s| s.status == "completed" && s.ended_at.is_some())
        .max_by_key(|s| parse_utc_ms(s.ended_at.as_deref().or(Some(s.started_at.as_str()))))?;

    // 时间窗：结束日必须在 7 天内（学习日口径）
    let ended_day = last
        .ended_at
        .as_deref()
        .map(|raw| study_day_of(raw))
        .unwrap_or_else(|| study_day_of(&last.started_at));
    if study_day_of(&last.started_at) < cutoff && ended_day < cutoff {
        return None;
    }

    let linked = last
        .task_id
        .and_then(|tid| snapshot.today_tasks.iter().find(|t| t.id == tid));
    let task_continuable = linked.map(|t| t.status != "completed").unwrap_or(false);

    let (payload_kind, task_id, learning_item_id) = if task_continuable {
        (
            "start_task".to_string(),
            linked.map(|t| t.id),
            linked.and_then(|t| t.learning_item_id),
        )
    } else if last.learning_item_id.is_some() {
        ("start_item".to_string(), None, last.learning_item_id)
    } else {
        ("start_quick".to_string(), None, None)
    };

    let minutes = last
        .duration_seconds
        .map(|s| (s / 60).max(0))
        .filter(|m| *m > 0);
    let mut reasons = vec!["你最近一次学习停在这里，接着学会更快进入状态。".to_string()];
    if let Some(m) = minutes {
        reasons.push(format!("上次实际学习 {} 分钟。", m));
    }
    reasons.push(if task_continuable {
        "关联任务仍未完成。".to_string()
    } else {
        "会新建一条学习记录，不会改动上次记录。".to_string()
    });

    Some(Candidate {
        action_type: NextActionType::ContinueLast,
        reason_code: REASON_CONTINUE_LAST.to_string(),
        source: ActionSource::Session {
            session_id: last.id,
        },
        title: if last.title.trim().is_empty() {
            "上次学习记录".to_string()
        } else {
            last.title.clone()
        },
        subtitle: minutes.map(|m| format!("上次学习 {} 分钟", m)),
        reasons,
        payload_kind,
        task_id,
        learning_item_id,
        session_id: Some(last.id),
        review_id: None,
        base_estimate: minutes,
        task_scoped: task_continuable,
        slice_by_design: false,
        deadline_minutes: None,
        priority_rank: 3,
        minutes_fit: minutes_fit_of(minutes, available),
        recency: parse_utc_ms(last.ended_at.as_deref()),
        stable_id: last.id,
    })
}

fn planned_task_candidates(
    snapshot: &LearningStateSnapshot,
    available: Option<i64>,
) -> Vec<Candidate> {
    // 任务 id → 最近一次关联学习时间（迁移自 buildStartHereCandidates 的 taskRecency）
    let mut task_recency: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for s in &snapshot.recent_sessions {
        if s.profile_id != snapshot.profile_id {
            continue;
        }
        let tid = match s.task_id {
            Some(t) => t,
            None => continue,
        };
        let ms = parse_utc_ms(s.ended_at.as_deref().or(Some(s.started_at.as_str())));
        let cur = task_recency.entry(tid).or_insert(0);
        if ms > *cur {
            *cur = ms;
        }
    }

    // PHASE 4 §4.3「next action filtering」：Micro Evidence 参与候选排序与理由。
    //
    // M0-B：`recent_touched_sources` 的 `source_type / source_id` 是**trigger source truth**
    // （session 触发就是 session，task 触发就是 task），不得为了这里的便利被改写。
    // 因此这里只做**只读派生**解析，把它映射回对应的 Knowledge Item：
    //   learning_item → 自身                                   （直接 item 触发）
    //   session       → `recent_sessions[].learning_item_id`    （正式/快速学习的学习项）
    //   task          → `today_tasks[].learning_item_id`        （任务的学习项）
    //   evaluation / goal → 本快照不足以可靠解析 → 跳过（不猜测、不伪造）
    // 解析结果只用于 recency / 理由文案，绝不回写 Micro 的 source。
    let mut micro_touch: std::collections::HashMap<i64, (i64, String)> =
        std::collections::HashMap::new();
    for touch in &snapshot.micro.recent_touched_sources {
        let item_id = match touch.source_type.as_str() {
            "learning_item" => touch.source_id,
            "session" => touch.source_id.and_then(|sid| {
                snapshot
                    .recent_sessions
                    .iter()
                    .find(|s| s.id == sid && s.profile_id == snapshot.profile_id)
                    .and_then(|s| s.learning_item_id)
            }),
            "task" => touch.source_id.and_then(|tid| {
                snapshot
                    .today_tasks
                    .iter()
                    .find(|t| t.id == tid)
                    .and_then(|t| t.learning_item_id)
            }),
            _ => None,
        };
        let Some(item_id) = item_id else { continue };
        let ms = parse_utc_ms(Some(touch.last_completed_at.as_str()));
        let entry = micro_touch.entry(item_id).or_insert((ms, String::new()));
        if ms >= entry.0 {
            *entry = (ms, touch.last_result.clone());
        }
    }

    // M2 §M2-F 反锤击（deterministic）：High 摩擦主体在冷却期内**不得**因为
    // 「最近刚做过」而被再次抬高 —— 否则会形成「越卡越推荐、越推荐越卡」的锤击循环。
    //
    // 这里只丢弃 **recency 提升**，不丢弃任务本身：任务仍然是合法的今日计划项，
    // 只是不再额外插队。冷却结束后（或有新证据推翻 High）自动恢复。
    let now_utc = crate::learning_state::date::now_utc();
    micro_touch.retain(|item_id, _| {
        !snapshot
            .friction
            .is_subject_in_cooldown(Some(*item_id), &now_utc)
    });

    snapshot
        .today_tasks
        .iter()
        .filter(|t| t.status != "completed")
        .map(|t| {
            let core = t.priority == "core" && t.task_kind != "accumulation";
            let est = t.estimated_minutes;
            let mut reasons = vec![if core {
                "今天计划中优先级最高（核心）。".to_string()
            } else {
                "今天计划中的任务。".to_string()
            }];
            reasons.push(match t.planned_time {
                Some(ref pt) => format!("计划时间 {}。", pt),
                None => "未指定具体时间。".to_string(),
            });
            reasons.push(match est {
                Some(e) => format!("预计 {} 分钟。", e),
                None => "未设置预计时长，可自由安排。".to_string(),
            });
            // §4.3：刚发生过 Micro 的来源必须让「下一次推荐」可观察到变化。
            //
            // M0-D：`recent_touched_sources` 在 SQL 层就排除了 `skipped`
            // （`result IN ('done','partial')`），所以这里只可能有两种结果 ——
            // 不再保留「跳过」分支，避免出现暗示「用户被记录了一次跳过」的措辞。
            let touched = t
                .learning_item_id
                .and_then(|iid| micro_touch.get(&iid).cloned())
                .filter(|(ms, _)| *ms > 0);
            if let Some((_, ref result)) = touched {
                reasons.push(format!(
                    "你最近在这里做过一次 Micro 动作（{}）。",
                    if result == "partial" {
                        "部分完成"
                    } else {
                        "已完成"
                    }
                ));
            }
            let recency = task_recency
                .get(&t.id)
                .copied()
                .unwrap_or(0)
                .max(touched.map(|(ms, _)| ms).unwrap_or(0));
            Candidate {
                action_type: NextActionType::PlannedTask,
                reason_code: if core {
                    REASON_PLANNED_TASK_CORE.to_string()
                } else {
                    REASON_PLANNED_TASK.to_string()
                },
                source: ActionSource::Task { task_id: t.id },
                title: t.title.clone(),
                subtitle: est.map(|e| format!("预计 {} 分钟", e)),
                reasons,
                payload_kind: "start_task".to_string(),
                task_id: Some(t.id),
                learning_item_id: t.learning_item_id,
                session_id: None,
                review_id: None,
                base_estimate: est,
                task_scoped: true,
                slice_by_design: false,
                deadline_minutes: parse_planned_minutes(t.planned_time.as_deref()),
                priority_rank: priority_rank_of(t),
                minutes_fit: minutes_fit_of(est, available),
                recency,
                stable_id: t.id,
            }
        })
        .collect()
}

fn quick_study_candidate(available: Option<i64>) -> Candidate {
    let base = available.unwrap_or(QUICK_STUDY_DEFAULT_MINUTES).max(1);
    Candidate {
        action_type: NextActionType::QuickStudy,
        reason_code: REASON_QUICK_STUDY.to_string(),
        source: ActionSource::None,
        title: "快速学习".to_string(),
        subtitle: Some("不绑定任务，立刻开始计时".to_string()),
        reasons: vec![
            "不绑定任务、不要求填任何字段，点一下就开始计时。".to_string(),
            "结束后仍可补充任务与笔记。".to_string(),
        ],
        payload_kind: "start_quick".to_string(),
        task_id: None,
        learning_item_id: None,
        session_id: None,
        review_id: None,
        base_estimate: Some(base),
        task_scoped: false,
        slice_by_design: true,
        deadline_minutes: None,
        priority_rank: 9,
        minutes_fit: minutes_fit_of(Some(QUICK_STUDY_DEFAULT_MINUTES), available),
        recency: 0,
        stable_id: i64::MAX,
    }
}

/// raw SQLite datetime（UTC）→ 学习日（UTC+8）。
fn study_day_of(raw: &str) -> String {
    let normalized = if raw.contains('T') {
        raw.to_string()
    } else {
        format!("{}Z", raw.replace(' ', "T"))
    };
    match chrono::DateTime::parse_from_rfc3339(&normalized) {
        Ok(dt) => {
            let tz = chrono::FixedOffset::east_opt(8 * 3600).expect("UTC+8");
            dt.with_timezone(&tz).format("%Y-%m-%d").to_string()
        }
        Err(_) => String::new(),
    }
}

fn to_payload(c: &Candidate, budget: Option<TimeBudget>) -> (ExecutionPayload, Option<i64>, bool) {
    let (estimated, entry_slice) = apply_budget(c.base_estimate, budget, c.task_scoped);
    let payload = ExecutionPayload {
        kind: c.payload_kind.clone(),
        task_id: c.task_id,
        learning_item_id: c.learning_item_id,
        session_id: c.session_id,
        review_id: c.review_id,
        entry_slice,
        suggested_minutes: estimated.unwrap_or(0),
    };
    (payload, estimated, entry_slice)
}

/// M1-A：canonical 候选序列（**与 NextAction 完全同一套构造 + 同一套排序**）。
///
/// 唯一用途：让 `learning_state::pack` 复用同一份 primitive 源，
/// 杜绝「pack 第二套推荐引擎」（§15 / LP-06）。
/// 调用方**不得重排**，只允许截断与去重。
pub(crate) fn build_ranked_candidates(
    snapshot: &LearningStateSnapshot,
    available: Option<i64>,
) -> Vec<Candidate> {
    let today = snapshot.local_date.as_str();
    let mut candidates: Vec<Candidate> = Vec::new();
    if let Some(active) = active_session_candidate(snapshot) {
        candidates.push(active);
    }
    if let Some(rec) = recovery_candidate(snapshot) {
        candidates.push(rec);
    }
    if let Some(cl) = continue_last_candidate(snapshot, today, available) {
        candidates.push(cl);
    }
    if let Some(rv) = review_due_candidate(snapshot) {
        candidates.push(rv);
    }
    candidates.extend(planned_task_candidates(snapshot, available));
    candidates.push(quick_study_candidate(available));
    candidates.sort_by(compare_candidates);
    candidates
}

/// 候选 → 执行视图（payload / 建议分钟 / 是否入口切片）。
/// `pack` 通过它取得与 NextAction **完全一致** 的执行元数据（LP-06：前端无需自行推算）。
pub(crate) fn candidate_view(
    c: &Candidate,
    budget: Option<TimeBudget>,
) -> (ExecutionPayload, Option<i64>, bool) {
    to_payload(c, budget)
}

// =============== 正式入口 ===============

/// 生产入口：给定统一状态快照，返回**唯一** Primary 动作（+ 备选）。
pub fn build_next_learning_action(
    snapshot: &LearningStateSnapshot,
    budget: Option<TimeBudget>,
) -> Result<NextLearningAction, String> {
    // ---- M0-A：Micro 只在存在**真实 grounding 来源**候选时可用 ----
    //
    // `snapshot.micro.candidates` 全部由 `micro::generate_candidates` 从
    // evaluation / session / task / learning_item 等真实事实产出，并已完成 §4.3 去重。
    // 候选为空 ⇒ `Micro unavailable`：绝不伪造 micro、绝不调 Cloud 去「凑」一个。
    let micro_candidate: Option<MicroActionCandidate> = match budget {
        Some(bud) if bud.is_micro() => snapshot.micro.candidates.first().cloned(),
        _ => None,
    };
    // Micro 不可用时把 30 秒档降级成最小**真实**学时档（3 分钟），
    // 这样「Normal NextAction / Quick Study remains available」才是一个可执行动作
    // （30 秒档会被 `apply_budget` 钳成 0 分钟，产出无法执行的载荷）。
    let effective_budget = match budget {
        Some(bud) if bud.is_micro() && micro_candidate.is_none() => Some(bud.normal_fallback()),
        other => other,
    };
    let available = effective_budget.map(|b| b.minutes());

    let candidates = build_ranked_candidates(snapshot, available);

    // PHASE 2 硬规则：Active Session → Primary 必须 Continue Active
    if snapshot.active_session.is_some() {
        let active = candidates
            .iter()
            .find(|c| c.action_type == NextActionType::ActiveSession)
            .ok_or_else(|| "内部错误：active session 未生成候选".to_string())?
            .clone();
        return Ok(finish(active, Vec::new(), snapshot, effective_budget));
    }

    // PHASE 3：30 秒档 + **有 grounded Micro** → 只返回 micro_action（绝不创建普通 StudySession）
    if let Some(cand) = micro_candidate {
        let alternates = candidates
            .iter()
            .skip(1)
            .take(MAX_ALTERNATIVES)
            .map(|c| to_alternative(c, budget))
            .collect::<Vec<_>>();
        let context = candidates
            .first()
            .cloned()
            .unwrap_or_else(|| quick_study_candidate(available));
        return Ok(finish_micro(cand, context, alternates, snapshot, budget));
    }

    // 类别优先级为权威（迁移规则）；在同一类别内优先选择能完整放入时间档的候选。
    let primary_idx = match effective_budget {
        Some(bud) => {
            let top_cat = candidates[0].action_type.category_rank();
            candidates
                .iter()
                .position(|c| c.action_type.category_rank() == top_cat && fits_budget(c, bud))
                .unwrap_or(0)
        }
        None => 0,
    };
    let primary = candidates[primary_idx].clone();

    let alternates = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != primary_idx)
        .take(MAX_ALTERNATIVES)
        .map(|(_, c)| to_alternative(c, effective_budget))
        .collect::<Vec<_>>();

    let mut action = finish(primary, alternates, snapshot, effective_budget);

    // M0-A：用户选了 30 秒档但没有任何 grounded Micro → 如实说明「Micro 不可用」，
    // 并把它替换成最小真实学时档。不允许静默改变用户看到的时间语义。
    if matches!(budget, Some(b) if b.is_micro()) && micro_candidate.is_none() {
        action.reason_code = REASON_MICRO_UNAVAILABLE.to_string();
        action.reasons.insert(0, MICRO_UNAVAILABLE_REASON.to_string());
    }

    Ok(action)
}

fn to_alternative(c: &Candidate, budget: Option<TimeBudget>) -> NextActionAlternative {
    let (payload, estimated, entry_slice) = to_payload(c, budget);
    let reason_code = if entry_slice {
        format!("{}_slice", c.reason_code)
    } else {
        c.reason_code.clone()
    };
    NextActionAlternative {
        action_type: c.action_type,
        reason_code,
        source_entity: c.source.clone(),
        estimated_minutes: estimated,
        execution_payload: payload,
        title: c.title.clone(),
        subtitle: c.subtitle.clone(),
        reasons: c.reasons.clone(),
    }
}

/// 组装普通（非 micro）Primary 动作。
///
/// **M0-A**：本函数不再承担任何「伪造 Micro」的职责 —— 旧签名里的 `micro: bool`
/// 分支会在无 grounded 候选时凭 recovery / review 状态凭空造一个 micro，
/// 已被删除。Micro 的组装统一走 [`finish_micro`]，且只在
/// `snapshot.micro.candidates.first()` 真实存在时才会被调用。
fn finish(
    primary: Candidate,
    alternates: Vec<NextActionAlternative>,
    snapshot: &LearningStateSnapshot,
    budget: Option<TimeBudget>,
) -> NextLearningAction {
    let (payload, estimated, entry_slice) = to_payload(&primary, budget);
    let reason_code = if entry_slice {
        REASON_PLANNED_TASK_SLICE.to_string()
    } else {
        primary.reason_code.clone()
    };

    NextLearningAction {
        profile_id: snapshot.profile_id,
        local_date: snapshot.local_date.clone(),
        action_type: primary.action_type,
        reason_code,
        source_entity: primary.source.clone(),
        estimated_minutes: estimated,
        source_task_estimate_minutes: primary.task_id.and(primary.base_estimate),
        available_minutes: budget.map(|b| b.minutes()),
        execution_payload: payload,
        title: primary.title.clone(),
        subtitle: primary.subtitle.clone(),
        reasons: primary.reasons.clone(),
        is_primary: true,
        micro_action_only: false,
        micro_action: None,
        alternates,
    }
}

/// 组装 30 秒档的 Micro Primary（PHASE 3 / PHASE 4）。
///
/// **前置条件（M0-A）：`cand` 一定来自 `snapshot.micro.candidates`，即一定有真实 grounding 来源。**
/// 载荷刻意不携带任何「可开始 Session」的目标：来源只通过 `source_entity` 与
/// `micro_action` 表达，UI 无从误开 StudySession（§4.5）。
fn finish_micro(
    cand: MicroActionCandidate,
    context: Candidate,
    alternates: Vec<NextActionAlternative>,
    snapshot: &LearningStateSnapshot,
    budget: Option<TimeBudget>,
) -> NextLearningAction {
    NextLearningAction {
        profile_id: snapshot.profile_id,
        local_date: snapshot.local_date.clone(),
        action_type: context.action_type,
        reason_code: REASON_MICRO_ACTION.to_string(),
        source_entity: micro_source_entity(&cand),
        estimated_minutes: Some(0),
        source_task_estimate_minutes: context.task_id.and(context.base_estimate),
        available_minutes: budget.map(|b| b.minutes()),
        execution_payload: ExecutionPayload {
            kind: "micro_action".to_string(),
            task_id: None,
            learning_item_id: None,
            session_id: None,
            review_id: None,
            entry_slice: false,
            suggested_minutes: 0,
        },
        title: cand.title.clone(),
        subtitle: Some(format!(
            "micro action · {} · {}",
            cand.action_type, cand.prompt_variant
        )),
        reasons: vec![
            cand.reason.clone(),
            format!(
                "只做这一小步（约 {} 秒），不会创建学习记录。",
                cand.estimated_seconds
            ),
        ],
        is_primary: true,
        micro_action_only: true,
        micro_action: Some(cand),
        alternates,
    }
}

/// Micro 候选来源 → 既有的强类型 `ActionSource`。
///
/// `task` / `session` / `learning_item` 能映射到既有枚举；`evaluation` / `goal` / `none`
/// 在 `ActionSource` 中没有对应变体 —— 完整来源始终由 `micro_action.source_type /
/// source_id` 表达（弱引用），因此这里返回 `None` 而不是硬塞一个错误语义。
fn micro_source_entity(cand: &MicroActionCandidate) -> ActionSource {
    match (cand.source_type.as_str(), cand.source_id) {
        ("task", Some(id)) => ActionSource::Task { task_id: id },
        ("session", Some(id)) => ActionSource::Session { session_id: id },
        ("learning_item", Some(id)) => ActionSource::LearningItem {
            learning_item_id: id,
        },
        _ => ActionSource::None,
    }
}

/// 便于单测的可读断言：Primary 的唯一性不变量（exactly one primary）。
pub fn assert_single_primary(action: &NextLearningAction) -> Result<(), String> {
    if !action.is_primary {
        return Err("主推荐必须标记 is_primary".into());
    }
    if action.alternates.len() > MAX_ALTERNATIVES {
        return Err(format!("备选超过 {} 条", MAX_ALTERNATIVES));
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn parse_planned_minutes_migrated_semantics() {
        assert_eq!(parse_planned_minutes(Some("09:30")), Some(570));
        assert_eq!(parse_planned_minutes(Some("9:05")), Some(545));
        assert_eq!(parse_planned_minutes(Some("24:00")), None);
        assert_eq!(parse_planned_minutes(Some("09:70")), None);
        assert_eq!(parse_planned_minutes(None), None);
    }

    #[test]
    fn parse_utc_ms_handles_sqlite_and_iso() {
        let a = parse_utc_ms(Some("2026-09-15 01:00:00"));
        let b = parse_utc_ms(Some("2026-09-15T01:00:00Z"));
        assert!(a > 0);
        assert_eq!(a, b);
        assert_eq!(parse_utc_ms(None), 0);
        assert_eq!(parse_utc_ms(Some("garbage")), 0);
    }

    #[test]
    fn apply_budget_never_exceeds_available() {
        let (est, slice) = apply_budget(Some(25), Some(TimeBudget::Min3), true);
        assert_eq!(est, Some(3));
        assert!(slice, "25 分钟任务在 3 分钟档必须降级为入口切片");
        let (est2, slice2) = apply_budget(Some(3), Some(TimeBudget::Min3), true);
        assert_eq!(est2, Some(3));
        assert!(!slice2, "刚好放得下就不是切片");
        let (est3, slice3) = apply_budget(None, Some(TimeBudget::Min10), true);
        assert_eq!(est3, Some(10));
        assert!(slice3);
        let (est4, _) = apply_budget(Some(25), None, true);
        assert_eq!(est4, Some(25));
    }
}
