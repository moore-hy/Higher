//! DEV-0077.4-A.1 · Grounding / Atomicity Validator（§四十一/§五十六-§五十八/§九十六/§六一）。
//!
//! §六一：Prompt 不能代替 Validator——即使 Prompt 写了规则，Backend 仍必须校验。
//! 全部 pure（无 DB）：图校验 + Task 原子性 + dangling ref + Repair 指令生成。

use std::collections::HashSet;

use super::types::{GroundingCompleteness, LearningUnitDraft, TaskGroundingDraft, TaskGroundingMode};

/// §四十一：Task 原子性。Learning → 恰 1 unit；Meta → 0 unit。
/// 返回 Err(人类可读修复指令素材)。
pub fn validate_task_atomicity(task_title: &str, g: &TaskGroundingDraft) -> Result<(), String> {
    match g.mode {
        TaskGroundingMode::Learning => {
            if g.unit_refs.len() == 1 {
                Ok(())
            } else if g.unit_refs.is_empty() {
                Err(format!(
                    "学习任务「{task_title}」缺少学习单元（unit_refs 为空）。\
                     必须恰好关联 1 个 learning_unit；若是非学习杂务请改用 mode=meta。"
                ))
            } else {
                Err(format!(
                    "学习任务「{task_title}」关联了 {} 个学习单元（{}），违反任务原子性：\
                     一个 Task 只能有一个 Primary Learning Unit，请拆分为多个 Task（并重新分配 estimated_minutes）",
                    g.unit_refs.len(),
                    g.unit_refs.join(", ")
                ))
            }
        }
        TaskGroundingMode::Meta => {
            if g.unit_refs.is_empty() {
                Ok(())
            } else {
                Err(format!(
                    "杂务任务「{task_title}」标记为 meta 但携带 unit_refs（{}）；\
                     meta 任务不得关联学习单元（若实际是学习任务请改 mode=learning 并只留 1 个）",
                    g.unit_refs.join(", ")
                ))
            }
        }
    }
}

/// §九十六：Unit Graph 校验（pure）：ref_key 唯一非空 / parent_ref 存在 / 无环 / 名合法。
pub fn validate_unit_graph(units: &[LearningUnitDraft]) -> Vec<String> {
    let mut errors = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for u in units {
        if u.ref_key.trim().is_empty() {
            errors.push(format!("learning_unit「{}」缺少 ref_key", u.name));
        }
        if !seen.insert(u.ref_key.trim()) {
            errors.push(format!("ref_key 重复：{}", u.ref_key));
        }
        if u.name.trim().is_empty() {
            errors.push(format!("learning_unit「{}」名称为空", u.ref_key));
        }
        // §六概念冻结：Unit 是知识单位，不是日期型活动
        if is_dated_activity_name(&u.name) {
            errors.push(format!(
                "「{}」是日期/活动型名称，不是可复用知识单位（LearningUnit 禁止）",
                u.name
            ));
        }
    }
    let by_ref: HashSet<&str> = units.iter().map(|u| u.ref_key.trim()).collect();
    for u in units {
        let p = u.parent_ref.trim();
        if !p.is_empty() && !by_ref.contains(p) {
            errors.push(format!(
                "learning_unit「{}」的 parent_ref「{p}」不存在于本 Draft（dangling）",
                u.ref_key
            ));
        }
    }
    // 环检测（DFS）
    for start in units {
        let mut cur: Option<&LearningUnitDraft> = Some(start);
        let mut path: Vec<&str> = Vec::new();
        let mut guard = 0usize;
        while let Some(u) = cur {
            guard += 1;
            if guard > units.len() + 1 {
                errors.push(format!(
                    "learning_units 存在 parent 环（经过 {}）",
                    path.join(" → ")
                ));
                break;
            }
            path.push(&u.ref_key);
            let p = u.parent_ref.trim();
            cur = if p.is_empty() {
                None
            } else {
                units.iter().find(|x| x.ref_key.trim() == p)
            };
        }
    }
    errors
}

/// 日期/活动型名称防御（保守启发：含「日」+数字量词 或 今日/明日/复习计划 等）。
fn is_dated_activity_name(name: &str) -> bool {
    let n = name.trim();
    n.contains("今日") || n.contains("明日") || n.contains("今天") || n.contains("明天")
}

/// §四一+§九十六：一批 Task grounding 的统一校验（unit_refs 必须指向 Draft units）。
pub fn validate_task_groundings(
    tasks: &[( /* title */ String, /* grounding */ Option<&TaskGroundingDraft>)],
    units: &[LearningUnitDraft],
) -> Vec<String> {
    let mut errors = Vec::new();
    let unit_refs: HashSet<&str> = units.iter().map(|u| u.ref_key.trim()).collect();
    for (title, g) in tasks {
        match g {
            None => errors.push(format!(
                "任务「{title}」缺少 grounding 声明（learning 任务须 unit_refs=[1个]，杂务须 mode=meta）"
            )),
            Some(g) => {
                if let Err(e) = validate_task_atomicity(title, g) {
                    errors.push(e);
                }
                for r in &g.unit_refs {
                    if !unit_refs.contains(r.trim()) {
                        errors.push(format!(
                            "任务「{title}」的 unit_ref「{r}」不存在于 learning_units（dangling）"
                        ));
                    }
                }
            }
        }
    }
    errors
}

/// §四〇：Grounding Completeness（pure，自 Draft 统计）。
pub fn grounding_completeness(
    tasks: &[( /* grounding */ Option<&TaskGroundingDraft>)],
) -> GroundingCompleteness {
    let mut c = GroundingCompleteness::default();
    for g in tasks {
        match g {
            Some(g) if g.mode == TaskGroundingMode::Learning => {
                c.learning_task_count += 1;
                if g.unit_refs.len() == 1 {
                    c.grounded_learning_task_count += 1;
                } else {
                    c.invalid_unlinked_learning_task_count += 1;
                }
            }
            Some(_) => c.meta_task_count += 1,
            None => {
                // 无声明：按「应为学习但丢失」计（校验层会拒；此处只做统计口径）
                c.learning_task_count += 1;
                c.invalid_unlinked_learning_task_count += 1;
            }
        }
    }
    c.rate = if c.learning_task_count == 0 {
        1.0
    } else {
        c.grounded_learning_task_count as f64 / c.learning_task_count as f64
    };
    c
}

/// §四十二：Grounding Repair Pass 指令（只给 invalid tasks + required unit refs，
/// 只拆 Task，不重新生成 Final Goal/Blueprint/Phase/Milestone，防全计划漂移）。
pub fn grounding_repair_instruction(errors: &[String], unit_refs: &[String]) -> String {
    format!(
        "以下任务未通过学习关联（Grounding/Atomicity）校验：\n{}\n\n\
         可用学习单元 ref_key：{}\n\n\
         请只修复上述任务：每个学习任务恰好关联 1 个 unit（多学科的拆成多个 Task 并重新分配 \
         estimated_minutes，总和接近原预算）；非学习杂务标记 mode=meta 且 unit_refs=[]。\
         不要改动 Final Goal / Blueprint / Phase / Milestone / 其他任务。\
         严格返回 JSON：{{\"tasks\":[{{\"title\":\"...\",\"date\":\"YYYY-MM-DD\",\
         \"estimated_minutes\":60,\"task_kind\":\"structured\",\"priority\":\"normal\",\
         \"goal_ref\":\"\",\"grounding\":{{\"mode\":\"learning|meta\",\"unit_refs\":[\"ref\"]}}}}]}}",
        errors.join("\n"),
        unit_refs.join(", ")
    )
}
