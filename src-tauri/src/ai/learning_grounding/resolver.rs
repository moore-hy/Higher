//! DEV-0077.4-A.1 · Existing LearningItem Resolver（§二十-§二十五/§九十三-§九十五）。
//!
//! 匹配顺序固定（§二十）：
//! 1. Draft 自带可信 existing_learning_item_id（验证 profile 一致 + 实体存在）→ 复用；
//! 2. 先解析 parent（topological，§九十五）；
//! 3. 同 Profile + 同 Resolved Parent 下 normalized_name **精确**匹配；
//! 4. 唯一命中 → Reuse；5. 无命中 → Create；6. 多命中 → Ambiguous（整体 Err）。
//!
//! §二十一：禁止 fuzzy 自动合并（embedding / LIKE / Levenshtein / LLM「看起来差不多」）。
//! §二十五：跨 Profile reuse = P0——index 只装载当前 Profile。
//! §九十三：**一次批量读取**当前 Profile 全树，建 (parent_id, normalized_name) 索引，
//! O(n) resolve，禁止每 Task 单独 query。

use std::collections::HashMap;

use rusqlite::{params, Connection};

use super::normalization::normalize_name;
use super::types::{CreateUnit, GroundingKey, GroundingResolution, LearningUnitDraft, ParentSpec};

/// §九十四：一次构建的 Grounding Index（profile 内）。
#[derive(Debug, Default)]
pub struct GroundingIndex {
    /// (parent_id, normalized_name) → ids（Vec 用于暴露历史重复 → ambiguity）。
    index: HashMap<GroundingKey, Vec<i64>>,
    /// id → profile 内存在性（trusted existing 校验用）。
    existing: HashMap<i64, ()>,
}

impl GroundingIndex {
    /// §九十三：批量装载当前 Profile 全树（1 条 SELECT）。
    pub fn load(conn: &Connection, profile_id: i64) -> Result<Self, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id, parent_id, name FROM learning_items
                 WHERE profile_id = ?1 ORDER BY id",
            )
            .map_err(|e| format!("learning_items 读取失败: {e}"))?;
        let rows = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, String>(2)?))
            })
            .map_err(|e| format!("learning_items 读取失败: {e}"))?;
        let mut me = Self::default();
        for row in rows {
            let (id, parent, name) = row.map_err(|e| format!("learning_items 读取失败: {e}"))?;
            me.existing.insert(id, ());
            me.index
                .entry(GroundingKey { parent_id: parent, normalized_name: normalize_name(&name) })
                .or_default()
                .push(id);
        }
        Ok(me)
    }

    /// §二十.3/§二十.6：同 Parent 精确匹配。Ok(None)=无命中（Create）；
    /// Ok(Some(id))=唯一命中（Reuse）；Err=Ambiguous（≥2 历史重复，不猜）。
    pub fn exact_match(&self, parent_id: Option<i64>, name: &str) -> Result<Option<i64>, String> {
        let key = GroundingKey { parent_id, normalized_name: normalize_name(name) };
        match self.index.get(&key).map(|v| v.as_slice()) {
            None | Some([]) => Ok(None),
            Some([only]) => Ok(Some(*only)),
            Some(ids) => Err(format!(
                "知识节点「{name}」在同父下存在 {} 个同名历史节点（ids={ids:?}），\
                 无法确定复用对象（ambiguity）；请先人工治理重复节点",
                ids.len()
            )),
        }
    }

    /// §二十.1：可信 existing id 校验（存在 + 本 Profile）。
    pub fn trusted_existing(&self, id: i64) -> bool {
        self.existing.contains_key(&id)
    }
}

/// §九十五：parent_ref 拓扑排序（父先子后）；dangling ref / cycle → Err。
/// 返回按依赖序排列的 (draft index) 序列。
pub fn topological_order(units: &[LearningUnitDraft]) -> Result<Vec<usize>, String> {
    let by_ref: HashMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.ref_key.as_str(), i))
        .collect();
    // 重复 ref_key 由 validator 先行拦截；此处容错取首个。
    let mut order: Vec<usize> = Vec::with_capacity(units.len());
    let mut visited: HashMap<usize, u8> = HashMap::new(); // 0=visiting 1=done
    fn visit(
        i: usize,
        units: &[LearningUnitDraft],
        by_ref: &HashMap<&str, usize>,
        visited: &mut HashMap<usize, u8>,
        order: &mut Vec<usize>,
        stack: &mut Vec<String>,
    ) -> Result<(), String> {
        match visited.get(&i) {
            Some(1) => return Ok(()),
            Some(0) => {
                return Err(format!(
                    "learning_units 存在 parent 环：{}",
                    stack.join(" → ")
                ))
            }
            _ => {}
        }
        visited.insert(i, 0);
        let parent_ref = units[i].parent_ref.trim();
        if !parent_ref.is_empty() {
            match by_ref.get(parent_ref) {
                Some(&p) => {
                    stack.push(units[i].ref_key.clone());
                    visit(p, units, by_ref, visited, order, stack)?;
                    stack.pop();
                }
                None => {
                    return Err(format!(
                        "learning_unit「{}」的 parent_ref「{parent_ref}」不存在于本 Draft（dangling）",
                        units[i].ref_key
                    ))
                }
            }
        }
        visited.insert(i, 1);
        order.push(i);
        Ok(())
    }
    // 共享 visited：0=visiting（环检测）1=done（剪枝）
    let mut visited: HashMap<usize, u8> = HashMap::new();
    for i in 0..units.len() {
        visit(i, units, &by_ref, &mut visited, &mut order, &mut Vec::new())?;
    }
    Ok(order)
}

/// §二十：完整解析。任何 ambiguity / dangling / cycle / 草稿内重复 → 整体 Err
/// （0 mutation；上层走 Repair 或 fail）。
pub fn resolve_grounding(
    conn: &Connection,
    profile_id: i64,
    units: &[LearningUnitDraft],
) -> Result<GroundingResolution, String> {
    let idx = GroundingIndex::load(conn, profile_id)?;
    let order = topological_order(units)?;
    let mut res = GroundingResolution::default();
    // ref_key → 已确定的真实 id（复用结果 / 同包前序 create 的 ref）
    // resolved_parent: Some(Ok(id))=复用父真实 id；Some(Err(ref))=同包 create 父；
    // None=Root。
    let mut resolved: HashMap<String, Result<Option<i64>, String>> = HashMap::new();
    let mut pending: HashMap<(String, String), String> = HashMap::new(); // 草稿内去重（§五十七前置）

    for &i in &order {
        let u = &units[i];
        // §二十.1：可信 existing 优先
        if let Some(id) = u.existing_learning_item_id {
            if idx.trusted_existing(id) {
                resolved.insert(u.ref_key.clone(), Ok(Some(id)));
                res.reuse.insert(u.ref_key.clone(), id);
                continue;
            }
            return Err(format!(
                "learning_unit「{}」携带的 existing_learning_item_id={id} 不存在于当前 Profile（跨 Profile 引用 = P0 禁止）",
                u.ref_key
            ));
        }
        // 父解析
        let parent_spec: ParentSpec = if u.parent_ref.trim().is_empty() {
            ParentSpec::Root
        } else {
            match resolved.get(u.parent_ref.trim()) {
                Some(Ok(Some(pid))) => ParentSpec::Existing(*pid),
                Some(Ok(None)) => ParentSpec::Root, // 父是 Root 复用 → 子挂 Root 下？不可能：父必为具体节点
                Some(Err(pref)) => ParentSpec::PackRef(pref.clone()),
                None => {
                    return Err(format!(
                        "learning_unit「{}」的 parent_ref「{}」未解析（dangling）",
                        u.ref_key, u.parent_ref
                    ))
                }
            }
        };
        let parent_for_key = match &parent_spec {
            ParentSpec::Root => None,
            ParentSpec::Existing(pid) => Some(*pid),
            ParentSpec::PackRef(_) => {
                // 同包父尚未落库 → 精确匹配无法以「父真实 id」查（父是新建的）。
                // 此时若 DB 在「该 pack 父的现有同名占位」下已有同 (None 父, 同名) 节点，
                // 属于模型把已有节点重复声明——保守策略：直接 Create（挂 pack ref），
                // 宁可少量重复待治理，绝不错误合并（§二十一）。
                None
            }
        };
        // 草稿内同 (parent, name) 重复 → 拒绝（防一次生成重复节点；§五十七前置）
        // parent 键：root / id:N / ref:K（区分 Root 与同包 create 父）
        let parent_tag = match &parent_spec {
            ParentSpec::Root => "root".to_string(),
            ParentSpec::Existing(pid) => format!("id:{pid}"),
            ParentSpec::PackRef(r) => format!("ref:{r}"),
        };
        let draft_key = (parent_tag, normalize_name(&u.name));
        if let Some(prev) = pending.get(&draft_key) {
            return Err(format!(
                "learning_units 草稿内重复：「{}」与「{}」在同级同名（{}）",
                prev,
                u.ref_key,
                u.name
            ));
        }
        match idx.exact_match(parent_for_key, &u.name) {
            Ok(Some(id)) => {
                // 唯一命中 → Reuse（不产生 op；Undo 绝不触碰）
                resolved.insert(u.ref_key.clone(), Ok(Some(id)));
                res.reuse.insert(u.ref_key.clone(), id);
                pending.insert(draft_key, u.ref_key.clone());
            }
            Ok(None) => {
                resolved.insert(u.ref_key.clone(), Err(u.ref_key.clone()));
                pending.insert(draft_key, u.ref_key.clone());
                res.create.push(CreateUnit {
                    ref_key: u.ref_key.clone(),
                    name: u.name.trim().to_string(),
                    description: u.description.clone().filter(|d| !d.trim().is_empty()),
                    goal_ref: u.goal_ref.clone().filter(|g| !g.trim().is_empty()),
                    parent: parent_spec,
                });
            }
            Err(e) => return Err(e),
        }
    }
    Ok(res)
}
