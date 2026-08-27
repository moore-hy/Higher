//! DEV-0070 Phase F v2.0 §7/§10 · 用户理解模块。
//!
//! - `UserContext`：AI 对用户的长期理解（§7 七字段，Serialize/Deserialize 支持 JSON 保存）。
//! - `analyze_document`：确定性解析主路径（模板节 + kv；不依赖 LLM，同步安全）。
//! - `analyze_with_provider`：§10 允许的 LLM 增强路径（Document → LLM 理解 →
//!   结构化 JSON → UserContext）；Provider 缺失/失败时回退确定性解析，
//!   生产上传流程当前走确定性路径（同步 command 不阻塞），LLM 接线点已留。

use serde::{Deserialize, Serialize};

/// §7 用户理解模型（长期信息，存 personalization_profiles.user_context_json）。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserContext {
    pub basic_information: Option<String>,
    pub current_status: Option<String>,
    pub abilities: Vec<String>,
    pub resources: Vec<String>,
    pub constraints: Vec<String>,
    pub preferences: Vec<String>,
    pub long_term_goals: Vec<String>,
}

impl UserContext {
    /// 七字段是否全部为空。
    pub fn is_empty(&self) -> bool {
        self.basic_information.as_deref().map(str::trim).unwrap_or("").is_empty()
            && self.current_status.as_deref().map(str::trim).unwrap_or("").is_empty()
            && self.long_term_goals.is_empty()
            && self.abilities.is_empty()
            && self.resources.is_empty()
            && self.constraints.is_empty()
            && self.preferences.is_empty()
    }

    /// 非空字段数。
    pub fn filled_fields(&self) -> usize {
        [
            !self.basic_information.as_deref().map(str::trim).unwrap_or("").is_empty(),
            !self.current_status.as_deref().map(str::trim).unwrap_or("").is_empty(),
            !self.long_term_goals.is_empty(),
            !self.abilities.is_empty(),
            !self.resources.is_empty(),
            !self.constraints.is_empty(),
            !self.preferences.is_empty(),
        ]
        .iter().filter(|b| **b).count()
    }

    /// 可读摘要（§15「当前用户理解」内容）。
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(b) = self.basic_information.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("基础信息：{b}"));
        }
        if let Some(s) = self.current_status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            parts.push(format!("当前状态：{s}"));
        }
        if !self.long_term_goals.is_empty() {
            parts.push(format!("长期目标：{}", self.long_term_goals.join("；")));
        }
        if !self.abilities.is_empty() {
            parts.push(format!("能力基础：{}", self.abilities.join("；")));
        }
        if !self.resources.is_empty() {
            parts.push(format!("时间资源：{}", self.resources.join("；")));
        }
        if !self.constraints.is_empty() {
            parts.push(format!("限制条件：{}", self.constraints.join("；")));
        }
        if !self.preferences.is_empty() {
            parts.push(format!("偏好：{}", self.preferences.join("；")));
        }
        parts.join("｜")
    }
}

// ---------------- §19 模板（八节，含教育背景） ----------------

/// 下载文件名。
pub const TEMPLATE_FILE_NAME: &str = "Higher_User_Profile_Template.md";

/// 生成模板全文（§19 固定八节结构）。
pub fn generate_template() -> String {
    let mut s = String::new();
    s.push_str("# 基础信息\n\n姓名：\n\n年龄：\n\n");
    s.push_str("# 当前状态\n\n目前身份：\n\n当前职业：\n\n");
    s.push_str("# 教育背景\n\n学历：\n\n毕业院校：\n\n专业：\n\n");
    s.push_str("# 能力基础\n\n数学：\n\n英语：\n\n专业能力：\n\n");
    s.push_str("# 长期目标\n\n目标：\n\n目标时间：\n\n");
    s.push_str("# 时间资源\n\n每天投入时间：\n\n周末时间：\n\n");
    s.push_str("# 限制条件\n\n困难：\n\n限制：\n\n");
    s.push_str("# 偏好\n\n学习方式：\n\nAI使用方式：\n");
    s
}

// ---------------- 确定性解析 ----------------

/// 模板节标题 → 字段桶（§19 八节；教育背景并入基础信息桶）。
const SECTION_MAP: &[(&str, &str)] = &[
    ("基础信息", "basic"),
    ("当前状态", "status"),
    ("教育背景", "basic"),
    ("能力基础", "abilities"),
    ("长期目标", "goals"),
    ("时间资源", "resources"),
    ("限制条件", "constraints"),
    ("偏好", "preferences"),
];

/// §9/§10 解析主路径：资料文本 → UserContext（确定性，无 LLM、无 DB 写入）。
pub fn analyze_document(text: &str) -> UserContext {
    let mut basic: Vec<String> = Vec::new();
    let mut status: Vec<String> = Vec::new();
    let mut goals: Vec<String> = Vec::new();
    let mut abilities: Vec<String> = Vec::new();
    let mut resources: Vec<String> = Vec::new();
    let mut constraints: Vec<String> = Vec::new();
    let mut preferences: Vec<String> = Vec::new();

    let mut section = "";
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            let title = line.trim_start_matches('#').trim();
            section = SECTION_MAP
                .iter()
                .find(|(name, _)| title.contains(name))
                .map(|(_, key)| *key)
                .unwrap_or("");
            continue;
        }
        let (key, value) = split_kv(line);
        let Some(v) = value else {
            // 无冒号的非空行：当作该节一条事实（长期目标常见裸行）
            match section {
                "basic" => basic.push(line.to_string()),
                "status" => status.push(line.to_string()),
                "goals" => goals.push(line.to_string()),
                "abilities" => abilities.push(line.to_string()),
                "resources" => resources.push(line.to_string()),
                "constraints" => constraints.push(line.to_string()),
                "preferences" => preferences.push(line.to_string()),
                _ => {}
            }
            continue;
        };
        if v.is_empty() {
            continue; // 模板空槽不算已填
        }
        let entry = if key.is_empty() { v.to_string() } else { format!("{key}：{v}") };
        match section {
            "basic" => basic.push(entry),
            "status" => status.push(entry),
            "goals" => goals.push(entry),
            "abilities" => abilities.push(entry),
            "resources" => resources.push(entry),
            "constraints" => constraints.push(entry),
            "preferences" => preferences.push(entry),
            _ => basic.push(entry), // 节外键值行 → 基础信息（尽力保留）
        }
    }

    UserContext {
        basic_information: join_non_empty(&basic),
        current_status: join_non_empty(&status),
        abilities: dedup_trim(abilities),
        resources: dedup_trim(resources),
        constraints: dedup_trim(constraints),
        preferences: dedup_trim(preferences),
        long_term_goals: dedup_trim(goals),
    }
}

/// F21-01/F22-02 · 正式 AI UserContext Analyzer（唯一正式写库来源）。
///
/// 输入必须是**完整 Profile Corpus**（本 profile 全部有效 sources 按 id ASC
/// 拼接，见 `build_profile_corpus`），整批只调用一次。
/// 经 ModelResponder（生产 Live=Primary AI；测试 Scripted=固定 structured result，
/// 禁止真实 Provider）输出严格校验的 UserContext：
/// - Provider 失败 / 非法 JSON / 空理解（七字段全空）→ Err；
/// - **Corpus 超出输入上限 → Err（拒绝截断后假装完整分析，F22-02）**；
/// - **绝不回退 analyze_document**——确定性 parser 不是正式 AI UserContext
///   的写库来源（失败路径由调用方标记 analysis_failed / dirty，不伪造完成）。
pub const CORPUS_MAX_CHARS: usize = 60_000;

pub async fn analyze_strict(
    responder: &crate::ai::agent::ModelResponder,
    corpus: &str,
) -> Result<UserContext, String> {
    if corpus.chars().count() > CORPUS_MAX_CHARS {
        return Err(format!(
            "Profile Corpus 超出 Provider 可接受输入范围（{} > {} chars），拒绝截断后假装完整分析",
            corpus.chars().count(),
            CORPUS_MAX_CHARS
        ));
    }
    let sys = "你是 Higher AI 的用户档案理解器。阅读用户资料，只输出一个 JSON 对象（不要 markdown 代码块、不要解释），\
字段：basic_information(string|null)、current_status(string|null)、abilities(string[])、\
resources(string[])、constraints(string[])、preferences(string[])、long_term_goals(string[])。\
资料中没有的信息用 null 或空数组，不要编造。";
    let messages = vec![
        crate::ai::client::ChatMessage::system(sys),
        crate::ai::client::ChatMessage::user(corpus.to_string()),
    ];
    let comp = responder.chat(messages, None, Some(2048)).await?;
    let raw = comp.content.unwrap_or_default().trim().to_string();
    let stripped = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(&raw)
        .trim_end_matches("```")
        .trim();
    let uc: UserContext = serde_json::from_str(stripped)
        .map_err(|e| format!("AI structured UserContext 非法 JSON：{e}"))?;
    if uc.is_empty() {
        return Err("AI structured UserContext 为空（七字段全空，不得写库）".to_string());
    }
    Ok(uc)
}

fn split_kv(line: &str) -> (&str, Option<&str>) {
    if let Some(idx) = line.find('：') {
        (&line[..idx], Some(line[idx + '：'.len_utf8()..].trim()))
    } else if let Some(idx) = line.find(':') {
        (&line[..idx], Some(line[idx + 1..].trim()))
    } else {
        (line, None)
    }
}

fn join_non_empty(items: &[String]) -> Option<String> {
    if items.is_empty() { None } else { Some(items.join("；")) }
}

fn dedup_trim(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for it in items {
        let t = it.trim().to_string();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}
