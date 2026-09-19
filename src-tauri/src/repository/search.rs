//! Higher 全局搜索（DEV-0052 / PHASE E §40-46）。
//!
//! SQLite FTS5（外部内容表 search_fts ← search_index，三触发器同步）。
//! 磁盘检索；无常驻向量库/外部服务。强制 profile_id 隔离。
//! 业务写入方调用 upsert/remove 维护索引（Goal/Task/Session/Knowledge/Document/
//! Evaluation/Memory/AiMessage/PersonalizationChunk）。

use rusqlite::{params, Connection};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub entity_type: String,
    pub entity_id: i64,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
    pub timestamp: Option<String>,
    pub deep_link: String,
}

pub struct SearchRepository<'a> {
    conn: &'a Connection,
}

/// FTS5 匹配表达式：逐词短语 OR（中文无空格时整句作为一个词；多词时任一命中即召回）。
///
/// 抽成纯函数是因为它现在有**两个**调用点（通用检索 / 受限检索），
/// 而两者必须共享**逐字相同**的匹配语义 —— 否则受限检索会悄悄变成另一个引擎。
fn build_match_expr(q: &str) -> String {
    let words: Vec<String> = q
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
        .collect();
    let match_words: Vec<String> = if words.is_empty() {
        vec![format!("\"{}\"", q.replace('"', "\"\""))]
    } else {
        words
    };
    match_words.join(" OR ")
}

/// 实体类型过滤的两种形态：FTS 路径带表别名，LIKE 回退路径不带。
fn type_filter_sql(entity_types: Option<&[String]>) -> (String, String) {
    match entity_types {
        Some(ts) if !ts.is_empty() => {
            let quoted: Vec<String> = ts
                .iter()
                .map(|t| format!("'{}'", t.replace('\'', "")))
                .collect();
            let list = quoted.join(",");
            (
                format!(" AND si.entity_type IN ({list})"),
                format!(" AND entity_type IN ({list})"),
            )
        }
        _ => (String::new(), String::new()),
    }
}

/// 授权 revision 范围的 SQL 片段（FTS 路径用 `EXISTS` 关联到既有文档结构表）。
///
/// `?base+0 … ?base+n-1` 是**占位符**，值由调用方绑定 —— 绝不把 id 拼进字符串。
///
/// 关键点：这个片段进的是 `WHERE`，因此在 `ORDER BY … LIMIT` **之前**生效。
/// 这正是「来源作用域必须先于 top-k」的落地方式。
fn revision_scope_fts(base_param: usize, revision_ids: &[i64]) -> String {
    let placeholders: Vec<String> = (0..revision_ids.len())
        .map(|i| format!("?{}", base_param + i))
        .collect();
    format!(
        " AND EXISTS (SELECT 1 FROM document_chunks c \
           JOIN document_revisions r ON r.id = c.revision_id \
          WHERE c.id = si.entity_id \
            AND c.profile_id = si.profile_id \
            AND r.profile_id = si.profile_id \
            AND r.id IN ({}))",
        placeholders.join(",")
    )
}

/// 同一个授权范围的 LIKE 回退形态。
///
/// CJK 回退**必须**应用**完全相同**的范围（P1.4 / OM-P1-12 / OM-P1-19），
/// 否则「FTS 路径被限制、回退路径没被限制」会成为一个静默的全库检索后门。
fn revision_scope_plain(prefix: &str, base_param: usize, revision_ids: &[i64]) -> String {
    let placeholders: Vec<String> = (0..revision_ids.len())
        .map(|i| format!("?{}", base_param + i))
        .collect();
    format!(
        " AND EXISTS (SELECT 1 FROM document_chunks c \
           JOIN document_revisions r ON r.id = c.revision_id \
          WHERE c.id = {prefix}.entity_id \
            AND c.profile_id = {prefix}.profile_id \
            AND r.profile_id = {prefix}.profile_id \
            AND r.id IN ({}))",
        placeholders.join(",")
    )
}

// ============================ FP-1 / FP-2：CJK 回退统一安全层 ============================

/// FP-2 —— CJK `LIKE` 回退的资源上限（确定性常量，禁止任意长度 query → 任意数量 OR）。
pub const MAX_CJK_FALLBACK_TERMS: usize = 12;
/// FP-2 —— 单个 CJK 回退 term 的最大字符数（中文按字符计，非字节）。
pub const MAX_CJK_FALLBACK_TERM_CHARS: usize = 64;

/// FP-1 —— **唯一**的 LIKE 安全处理实现（FTS 回退 / scoped 回退 / memory 回退 全部复用）。
///
/// 参数绑定只能防 SQL injection，不能阻止 `%` / `_` 继续作为 LIKE 通配符。
/// 采用 方案 B：把 LIKE 通配符与转义敏感字符统一转换为空格（普通安全字符），
/// 使其既不扩大匹配、也不改变词序。禁止在 FTS / CJK / scoped 三处各写一套 sanitize。
pub fn like_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '%' | '_' | '\\' => ' ',
            other => other,
        })
        .collect()
}

/// 截断到 `max` 个字符（中文按字符，避免按字节切一半）。
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

/// FP-2.1 / FP-2.2 —— CJK 回退 term 构建（确定性、有界、保序、fail-closed）。
///
/// raw query → split_whitespace → trim → 去空 → like_safe → 截断到
/// `MAX_CJK_FALLBACK_TERM_CHARS` → 再去空 → **保序**去重 → take(`MAX_CJK_FALLBACK_TERMS`)。
///
/// 返回空 Vec = 无有效 term → 调用方必须 fail closed（返回空结果，绝不退化成 `%`）。
pub fn cjk_like_terms(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in query.split_whitespace() {
        let term = truncate_chars(&like_safe(raw.trim()), MAX_CJK_FALLBACK_TERM_CHARS);
        let term = term.trim().to_string();
        if term.is_empty() {
            continue;
        }
        if !out.contains(&term) {
            out.push(term);
        }
        if out.len() >= MAX_CJK_FALLBACK_TERMS {
            break;
        }
    }
    out
}

impl<'a> SearchRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// upsert 索引行（触发器自动同步 FTS）。
    pub fn upsert(
        &self,
        entity_type: &str,
        entity_id: i64,
        profile_id: i64,
        title: &str,
        content: &str,
        timestamp: Option<&str>,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (entity_type, entity_id) DO UPDATE SET
               profile_id = excluded.profile_id,
               title = excluded.title,
               content = excluded.content,
               timestamp = excluded.timestamp",
            params![entity_type, entity_id, profile_id, title, content, timestamp],
        )?;
        Ok(())
    }

    pub fn remove(&self, entity_type: &str, entity_id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM search_index WHERE entity_type = ?1 AND entity_id = ?2",
            params![entity_type, entity_id],
        )?;
        Ok(())
    }

    /// §44 search_higher：FTS5 查询 + snippet + rank + deep_link；强制 profile 隔离。
    /// query 为空 → 返回空（不当作全量扫描通道）。
    /// DEV-0076 F.2 §三：Memory 命中二次校验——Search Index 只作候选，
    /// 数据库真实状态（memory_records.status='confirmed'）是最终授权判断；
    /// 历史/脏 FTS 中的 pending/rejected/dismissed/superseded 行不得返回。
    pub fn search(
        &self,
        profile_id: i64,
        query: &str,
        entity_types: Option<&[String]>,
        limit: i64,
    ) -> Result<Vec<SearchHit>, String> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // FTS5 匹配：逐词短语 OR（中文无空格时整句作为一个词；多词时任一命中即召回）
        let match_expr = build_match_expr(q);
        let (type_filter, type_filter_plain) = type_filter_sql(entity_types);
        let sql = format!(
            "SELECT si.entity_type, si.entity_id, si.title, si.timestamp,
                    snippet(search_fts, 1, '«', '»', '…', 24) AS snip,
                    bm25(search_fts) AS rank
             FROM search_fts fts
             JOIN search_index si ON si.rowid = fts.rowid
             WHERE search_fts MATCH ?1 AND si.profile_id = ?2{}
             ORDER BY rank
             LIMIT ?3",
            type_filter
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![match_expr, profile_id, limit.clamp(1, 100)], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, f64>(5)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let (etype, eid, title, ts, snip, rank) = row.map_err(|e| e.to_string())?;
            out.push(SearchHit {
                deep_link: deep_link_of(&etype, eid),
                entity_type: etype,
                entity_id: eid,
                title,
                snippet: snip,
                rank,
                timestamp: ts,
            });
        }
        // DEV-0076 F.2 §三：memory 实体二次授权（DB status = 事实源）
        out = self.filter_memory_hits(out, profile_id)?;
        if !out.is_empty() {
            return Ok(out);
        }
        // CJK fallback：unicode61 对中文按整段分 token（子串不可命中）→ LIKE 子串匹配。
        // 仍走 search_index 表（磁盘查询 + profile 隔离 + 类型过滤），非全表内存扫描。
        // FP-1/FP-2：多段 query 拆成有界 term 后 OR，统一经 `like_safe`；term 为空则
        // fail closed（返回空集，绝不退化成 `%` 全表命中）。
        let terms = cjk_like_terms(q);
        if !terms.is_empty() {
            let mut cond = Vec::new();
            let mut binds2: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(profile_id)];
            let mut p = 2usize;
            for term in &terms {
                let pat = format!("%{}%", term);
                cond.push(format!("(title LIKE ?{p} OR content LIKE ?{p})"));
                binds2.push(Box::new(pat));
                p += 1;
            }
            let cjk_limit_idx = p;
            binds2.push(Box::new(limit.clamp(1, 100)));
            let sql2 = format!(
                "SELECT entity_type, entity_id, title, timestamp FROM search_index
                 WHERE profile_id = ?1 AND ({}){}
                 ORDER BY entity_id DESC LIMIT ?{}",
                cond.join(" OR "),
                type_filter_plain,
                cjk_limit_idx
            );
            let mut stmt2 = self.conn.prepare(&sql2).map_err(|e| e.to_string())?;
            let rows2 = stmt2
                .query_map(
                    rusqlite::params_from_iter(binds2.iter().map(|b| b.as_ref())),
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;
            for row in rows2 {
                let (etype, eid, title, ts) = row.map_err(|e| e.to_string())?;
                out.push(SearchHit {
                    snippet: title.chars().take(80).collect(),
                    rank: 100.0,
                    deep_link: deep_link_of(&etype, eid),
                    entity_type: etype,
                    entity_id: eid,
                    title,
                    timestamp: ts,
                });
            }
        }
        // CJK fallback 同样过 memory 授权门（两路一致，§三）
        out = self.filter_memory_hits(out, profile_id)?;
        Ok(out)
    }

    /// GROUNDED LEARNING BRIDGE V1 · P1.4 —— 在**授权 revision 范围**内检索实体，
    /// 把范围写进 `WHERE`，即在 `ORDER BY … LIMIT` **之前**收敛候选集。
    ///
    /// # 为什么必须有这个入口
    ///
    /// [`Self::search`] 的契约是「先取 top-k，由调用方按来源过滤」。当同一 profile 内
    /// 存在 ≥k 条来自**别的来源**的高分命中时，目标来源的行根本进不了 top-k ——
    /// 「按来源过滤」这一步于是永远看不到它。过滤发生在截断之后，等于没有过滤。
    ///
    /// # 为什么这不是第二个引擎
    ///
    /// 同一张 `search_fts` / `search_index`、同一个 `bm25(search_fts)` 排名函数、
    /// 同一份 `build_match_expr` 匹配表达式、同一套 CJK `LIKE` 回退顺序。
    /// 唯一的差别是 `WHERE` 里多了一个由既有 v042 结构表派生的授权范围。
    /// 没有新表、没有新排名、没有内存重排。
    ///
    /// # fail closed
    ///
    /// `revision_ids` 为空 = **没有任何**授权 chunk，返回空集。
    /// 这里**绝不**退化成「无范围检索」—— 那正是调用方试图避免的全库检索。
    pub fn search_scoped_by_revisions(
        &self,
        profile_id: i64,
        entity_type: &str,
        query: &str,
        revision_ids: &[i64],
        limit: i64,
    ) -> Result<Vec<SearchHit>, String> {
        let q = query.trim();
        if q.is_empty() || revision_ids.is_empty() {
            return Ok(Vec::new());
        }

        let match_expr = build_match_expr(q);
        let limit = limit.clamp(1, 100);
        // ?1 = MATCH / ?2 = profile / ?3 = entity_type，之后是授权范围，最后是 LIMIT。
        const SCOPE_BASE: usize = 4;
        let limit_idx = SCOPE_BASE + revision_ids.len();

        let mut out: Vec<SearchHit> = Vec::new();

        // ---- 1) FTS5 路径：范围先于 top-k ----
        let sql = format!(
            "SELECT si.entity_type, si.entity_id, si.title, si.timestamp,
                    snippet(search_fts, 1, '«', '»', '…', 24) AS snip,
                    bm25(search_fts) AS rank
             FROM search_fts fts
             JOIN search_index si ON si.rowid = fts.rowid
             WHERE search_fts MATCH ?1 AND si.profile_id = ?2 AND si.entity_type = ?3{}
             ORDER BY rank
             LIMIT ?{limit_idx}",
            revision_scope_fts(SCOPE_BASE, revision_ids)
        );
        let mut binds: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(match_expr),
            Box::new(profile_id),
            Box::new(entity_type.to_string()),
        ];
        for id in revision_ids {
            binds.push(Box::new(*id));
        }
        binds.push(Box::new(limit));

        {
            let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    rusqlite::params_from_iter(binds.iter().map(|b| b.as_ref())),
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, f64>(5)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (etype, eid, title, ts, snip, rank) = row.map_err(|e| e.to_string())?;
                out.push(SearchHit {
                    deep_link: deep_link_of(&etype, eid),
                    entity_type: etype,
                    entity_id: eid,
                    title,
                    snippet: snip,
                    rank,
                    timestamp: ts,
                });
            }
        }

        if !out.is_empty() {
            out = self.filter_memory_hits(out, profile_id)?;
            return Ok(out);
        }

        // ---- 2) CJK 回退：**同一个**授权范围，同样先于 ORDER BY / LIMIT ----
        // FP-1/FP-2：多段 grounding query（item 名 + 描述 + goal + domain 拼接）必须拆成
        // 有界 term 后 OR，而非作为一整条 `%whole_query%` 字符串 —— 否则中文真实材料
        // 永远无法命中（整段子串不匹配）。统一经 `like_safe`；term 为空则 fail closed。
        let terms = cjk_like_terms(q);
        if !terms.is_empty() {
            let mut cond = Vec::new();
            let mut binds2: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(profile_id),
                Box::new(entity_type.to_string()),
            ];
            // ?1 = profile / ?2 = entity_type；revision 占位从 ?3 起。
            let mut p = 3usize;
            for id in revision_ids {
                binds2.push(Box::new(*id));
                p += 1;
            }
            for term in &terms {
                let pat = format!("%{}%", term);
                cond.push(format!("(si.title LIKE ?{p} OR si.content LIKE ?{p})"));
                binds2.push(Box::new(pat));
                p += 1;
            }
            let cjk_limit_idx = p;
            binds2.push(Box::new(limit));
            let sql2 = format!(
                "SELECT si.entity_type, si.entity_id, si.title, si.timestamp FROM search_index si
                 WHERE si.profile_id = ?1 AND ({}) AND si.entity_type = ?2{}
                 ORDER BY si.entity_id DESC LIMIT ?{}",
                cond.join(" OR "),
                revision_scope_plain("si", 3, revision_ids),
                cjk_limit_idx
            );
            let mut stmt2 = self.conn.prepare(&sql2).map_err(|e| e.to_string())?;
            let rows2 = stmt2
                .query_map(
                    rusqlite::params_from_iter(binds2.iter().map(|b| b.as_ref())),
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .map_err(|e| e.to_string())?;
            for row in rows2 {
                let (etype, eid, title, ts) = row.map_err(|e| e.to_string())?;
                out.push(SearchHit {
                    snippet: title.chars().take(80).collect(),
                    rank: 100.0,
                    deep_link: deep_link_of(&etype, eid),
                    entity_type: etype,
                    entity_id: eid,
                    title,
                    timestamp: ts,
                });
            }
        }
        out = self.filter_memory_hits(out, profile_id)?;
        Ok(out)
    }

    /// DEV-0076 F.2 §三：Memory 命中二次授权——对 hits 中 entity_type="memory"
    /// 的行，仅保留 memory_records 中 status='confirmed' 的（同 profile）。
    /// Search Index = 候选；数据库真实状态 = 最终授权（脏/历史 FTS 兜底）。
    fn filter_memory_hits(
        &self,
        mut hits: Vec<SearchHit>,
        profile_id: i64,
    ) -> Result<Vec<SearchHit>, String> {
        let needs = hits.iter().any(|h| h.entity_type == "memory");
        if !needs {
            return Ok(hits);
        }
        let confirmed: std::collections::HashSet<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM memory_records WHERE profile_id=?1 AND status='confirmed'")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![profile_id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<std::collections::HashSet<_>, _>>()
                .map_err(|e| e.to_string())?
        };
        hits.retain(|h| h.entity_type != "memory" || confirmed.contains(&h.entity_id));
        Ok(hits)
    }

    /// Memory 检索（§35 权重：相关度 × importance × confidence × recency × superseded × source）。
    pub fn search_memory(
        &self,
        profile_id: i64,
        query: &str,
        limit: i64,
    ) -> Result<Vec<i64>, String> {
        let q = query.trim();
        let fts_ids: Vec<i64> = if q.is_empty() {
            Vec::new()
        } else {
            let phrase = q.replace('"', "\"\"");
            let sql = "SELECT si.entity_id FROM search_fts fts
                 JOIN search_index si ON si.rowid = fts.rowid
                 WHERE search_fts MATCH ?1 AND si.profile_id = ?2 AND si.entity_type = 'memory'
                 ORDER BY bm25(search_fts) LIMIT 30";
            let ids: Vec<i64> = {
                let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
                let rows: Vec<i64> = stmt
                    .query_map(params![format!("\"{}\"", phrase), profile_id], |r| r.get(0))
                    .map_err(|e| e.to_string())?
                    .filter_map(|v| v.ok())
                    .collect();
                rows
            };
            ids
        };
        // 加权排序（相关命中 + 活跃 + 权重/置信/新近；两步法：候选再内存排序）
        // DEV-0076 §七：AI 检索口径 = confirmed（v027 后无 'active' 态）
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, memory_type, importance, confidence, created_at, status, source_kind
                 FROM memory_records
                 WHERE profile_id = ?1 AND status = 'confirmed'
                   AND (?2 = '' OR memory_key LIKE '%' || ?2 || '%' OR memory_value LIKE '%' || ?2 || '%')",
            )
            .map_err(|e| e.to_string())?;
        // FP-1：复用唯一共享的 LIKE 安全处理（与 FTS / scoped 回退同一实现）。
        let like = like_safe(q);
        let mut cands: Vec<(i64, String, i64, String, String, bool)> = Vec::new();
        let rows = stmt
            .query_map(params![profile_id, like], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(6)? == "user_message",
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            cands.push(row.map_err(|e| e.to_string())?);
        }
        // FTS 命中也并入候选
        for id in fts_ids {
            if !cands.iter().any(|c| c.0 == id) {
                let ok = self.conn.query_row(
                    "SELECT id, memory_type, importance, confidence, created_at, 1 FROM memory_records
                     WHERE id = ?1 AND profile_id = ?2 AND status = 'confirmed'",
                    params![id, profile_id],
                    |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, bool>(5)?,
                        ))
                    },
                );
                if let Ok(c) = ok {
                    cands.push(c);
                }
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let mut scored: Vec<(f64, i64)> = cands
            .into_iter()
            .map(|(id, mtype, importance, confidence, created, is_user)| {
                let mut s = importance as f64 * 2.0;
                s += match confidence.as_str() {
                    "high" => 3.0,
                    "medium" => 1.5,
                    _ => 0.5,
                };
                // 类型优先级（§35）：user explicit ≈ higher_db > opinion > inference
                s += match mtype.as_str() {
                    "user_fact" | "user_constraint" | "user_preference" | "goal_context" => 3.0,
                    "system_observation" => 3.0,
                    "user_opinion" => 1.5,
                    _ => 0.5, // ai_inference
                };
                if is_user {
                    s += 0.5;
                }
                // recency：每天衰减 0.02，下限 0.4
                let age_days = age_days_of(&created, now);
                s *= (1.0 - (age_days as f64) * 0.02).max(0.4);
                (s, id)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored
            .into_iter()
            .take(limit.clamp(1, 20) as usize)
            .map(|(_, id)| id)
            .collect())
    }
}

fn age_days_of(date_str: &str, now: i64) -> i64 {
    // SQLite datetime('now') 是 UTC "YYYY-MM-DD HH:MM:SS"
    if date_str.len() < 19 {
        return 0;
    }
    let y: i64 = date_str[0..4].parse().unwrap_or(2026);
    let mo: i64 = date_str[5..7].parse().unwrap_or(1);
    let d: i64 = date_str[8..10].parse().unwrap_or(1);
    let h: i64 = date_str[11..13].parse().unwrap_or(0);
    let mi: i64 = date_str[14..16].parse().unwrap_or(0);
    let s: i64 = date_str[17..19].parse().unwrap_or(0);
    let days = days_from_civil(y, mo, d);
    let secs = days * 86400 + h * 3600 + mi * 60 + s;
    ((now - secs) / 86400).max(0)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// §46 Deep Link。
pub fn deep_link_of(entity_type: &str, id: i64) -> String {
    match entity_type {
        "goal" => format!("higher://goal/{}", id),
        "task" => format!("higher://task/{}", id),
        "session" => format!("higher://session/{}", id),
        "knowledge" => format!("higher://knowledge/{}", id),
        "document" => format!("higher://document/{}", id),
        "memory" => format!("higher://memory/{}", id),
        "conversation" => format!("higher://conversation/{}", id),
        "evaluation" => format!("higher://evaluation/{}", id),
        "personalization_chunk" => "higher://personalization".to_string(),
        _ => format!("higher://{}/{}", entity_type, id),
    }
}

// ============ DEV-0057 PART L：Search Index 统一服务 ============
//
// §62-72 产品规则：Search Index = Canonical Data 的**派生搜索副本**（非正式数据）。
// 正式数据更新成功 → Index 必须同步；页面/AI/ChangeSet 不得各自维护一套。
// 本模块即该统一入口（SearchIndexService 等价物）：
//   - sync_entity_*：各实体在正式写路径后调用的同步函数（事务安全，可重入）
//   - rebuild_profile：从 Canonical tables 完整重建一个 profile 的索引
//   - ensure_index_version：settings KV `search.index.version`；缺失/变化才重建（§71-72）

/// 当前索引格式版本（语义变更时递增以触发一次性 rebuild）。
pub const SEARCH_INDEX_VERSION: &str = "2";

fn kv_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .ok()
}

fn kv_set(conn: &Connection, key: &str, value: &str) {
    let _ = conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    );
}

/// §69 rebuild：从 Canonical tables 完整重建当前 profile 全部索引（单事务；幂等）。
/// 返回（重建条数）。注意 memory/conversation/personalization_chunk 也在此重建
/// （它们本就有写路径维护；rebuild 保证一致性兜底）。
pub fn rebuild_profile(conn: &mut Connection, profile_id: i64) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM search_index WHERE profile_id = ?1",
        params![profile_id],
    )
    .map_err(|e| e.to_string())?;

    let mut n = 0usize;
    // goal（含 final；title=name，content=name+description）
    {
        let mut stmt = tx
            .prepare("SELECT id, name, COALESCE(description,'') FROM goals WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, name, desc) in rows {
            let content = if desc.is_empty() {
                name.clone()
            } else {
                format!("{name} {desc}")
            };
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('goal',?1,?2,?3,?4)",
                params![id, profile_id, name, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // task（title；content=title）
    {
        let mut stmt = tx
            .prepare("SELECT id, title FROM tasks WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String)> = stmt
            .query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, title) in rows {
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('task',?1,?2,?3,?3)",
                params![id, profile_id, title],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // session（title；content=title+note 纯文本截断 2000）
    {
        let mut stmt = tx
            .prepare("SELECT id, title, COALESCE(note,'') FROM study_sessions WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, title, note) in rows {
            let note: String = note.chars().take(2000).collect();
            let content = if note.is_empty() {
                title.clone()
            } else {
                format!("{title} {note}")
            };
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('session',?1,?2,?3,?4)",
                params![id, profile_id, title, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // knowledge（name；content=content 截断 2000）
    {
        let mut stmt = tx
            .prepare(
                "SELECT id, name, COALESCE(content,'') FROM learning_items WHERE profile_id=?1",
            )
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, name, content) in rows {
            let content: String = content.chars().take(2000).collect();
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('knowledge',?1,?2,?3,?4)",
                params![id, profile_id, name, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // document（title；content=content_text 截断 2000）
    {
        let mut stmt = tx
            .prepare("SELECT id, title, COALESCE(content_text,'') FROM knowledge_documents WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, title, text) in rows {
            let text: String = text.chars().take(2000).collect();
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('document',?1,?2,?3,?4)",
                params![id, profile_id, title, text],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // evaluation（title；content=title+note 截断）
    {
        let mut stmt = tx
            .prepare("SELECT id, title, COALESCE(note,'') FROM evaluations WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, title, note) in rows {
            let note: String = note.chars().take(1000).collect();
            let content = if note.is_empty() {
                title.clone()
            } else {
                format!("{title} {note}")
            };
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('evaluation',?1,?2,?3,?4)",
                params![id, profile_id, title, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // memory（memory_key；content=value）
    // DEV-0076 F.2 §二：通用索引只收 confirmed——pending_confirmation/
    // rejected/dismissed/superseded/draft 均不得进入 AI 可搜索索引
    //（FINAL AUDIT P0：rebuild 旁路泄漏未确认记忆）。
    {
        let mut stmt = tx
            .prepare("SELECT id, memory_key, memory_value FROM memory_records WHERE profile_id=?1 AND status='confirmed'")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, key, value) in rows {
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('memory',?1,?2,?3,?4)",
                params![id, profile_id, key, value],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // conversation（每条 assistant 消息全文；title=前 40 字）——仅 assistant（用户消息量太大）
    {
        let mut stmt = tx
            .prepare(
                "SELECT id, substr(content,1,40), content FROM ai_messages
                 WHERE profile_id=?1 AND role='assistant'",
            )
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, title, content) in rows {
            let content: String = content.chars().take(2000).collect();
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('conversation',?1,?2,?3,?4)",
                params![id, profile_id, title, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    // personalization chunk
    {
        let mut stmt = tx
            .prepare("SELECT id, chunk_index, content FROM personalization_source_chunks WHERE profile_id=?1")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, i64, String)> = stmt
            .query_map(params![profile_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|v| v.ok())
            .collect();
        for (id, idx, content) in rows {
            let title = format!("资料片段 #{idx}");
            let content: String = content.chars().take(2000).collect();
            tx.execute(
                "INSERT INTO search_index (entity_type, entity_id, profile_id, title, content) VALUES ('personalization_chunk',?1,?2,?3,?4)",
                params![id, profile_id, title, content],
            )
            .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

/// §71-72：索引版本门（不默认每次启动全重建）。
/// 当前 profile 的版本键缺失或不等于 SEARCH_INDEX_VERSION → 执行一次 rebuild 并落版本。
/// 返回 Some(rebuilt_count) 表示本次实际重建；None = 版本一致跳过。
pub fn ensure_index_version(
    conn: &mut Connection,
    profile_id: i64,
) -> Result<Option<usize>, String> {
    let key = format!("search.index.version.{profile_id}");
    if kv_get(conn, &key).as_deref() == Some(SEARCH_INDEX_VERSION) {
        return Ok(None);
    }
    let n = rebuild_profile(conn, profile_id)?;
    kv_set(conn, &key, SEARCH_INDEX_VERSION);
    Ok(Some(n))
}

// ---- §65-66 统一同步入口：正式写路径成功后调用 ----

/// task 同步（v1/v2 通用；status 变化不需要新索引，仅 title 相关）。
pub fn sync_task(conn: &Connection, profile_id: i64, task_id: i64) {
    let repo = SearchRepository::new(conn);
    if let Ok(row) = conn.query_row(
        "SELECT title FROM tasks WHERE id=?1 AND profile_id=?2",
        params![task_id, profile_id],
        |r| r.get::<_, String>(0),
    ) {
        let _ = repo.upsert("task", task_id, profile_id, &row, &row, None);
    }
}

pub fn remove_task(conn: &Connection, task_id: i64) {
    let _ = SearchRepository::new(conn).remove("task", task_id);
}

/// goal 同步。
pub fn sync_goal(conn: &Connection, profile_id: i64, goal_id: i64) {
    if let Ok((name, desc)) = conn.query_row(
        "SELECT name, COALESCE(description,'') FROM goals WHERE id=?1 AND profile_id=?2",
        params![goal_id, profile_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let content = if desc.is_empty() {
            name.clone()
        } else {
            format!("{name} {desc}")
        };
        let _ =
            SearchRepository::new(conn).upsert("goal", goal_id, profile_id, &name, &content, None);
    }
}

pub fn remove_goal(conn: &Connection, goal_id: i64) {
    let _ = SearchRepository::new(conn).remove("goal", goal_id);
}

/// knowledge item 同步。
pub fn sync_knowledge(conn: &Connection, profile_id: i64, item_id: i64) {
    if let Ok((name, content)) = conn.query_row(
        "SELECT name, COALESCE(content,'') FROM learning_items WHERE id=?1 AND profile_id=?2",
        params![item_id, profile_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let content: String = content.chars().take(2000).collect();
        let _ = SearchRepository::new(conn).upsert(
            "knowledge",
            item_id,
            profile_id,
            &name,
            &content,
            None,
        );
    }
}

pub fn remove_knowledge(conn: &Connection, item_id: i64) {
    let _ = SearchRepository::new(conn).remove("knowledge", item_id);
}

/// document 同步。
pub fn sync_document(conn: &Connection, profile_id: i64, doc_id: i64) {
    if let Ok((title, text)) = conn.query_row(
        "SELECT title, COALESCE(content_text,'') FROM knowledge_documents WHERE id=?1 AND profile_id=?2",
        params![doc_id, profile_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let text: String = text.chars().take(2000).collect();
        let _ = SearchRepository::new(conn).upsert("document", doc_id, profile_id, &title, &text, None);
    }
}

pub fn remove_document(conn: &Connection, doc_id: i64) {
    let _ = SearchRepository::new(conn).remove("document", doc_id);
}

/// session 同步（title/note 变化都刷新；note 截断 2000）。
pub fn sync_session(conn: &Connection, profile_id: i64, session_id: i64) {
    if let Ok((title, note)) = conn.query_row(
        "SELECT title, COALESCE(note,'') FROM study_sessions WHERE id=?1 AND profile_id=?2",
        params![session_id, profile_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let note: String = note.chars().take(2000).collect();
        let content = if note.is_empty() {
            title.clone()
        } else {
            format!("{title} {note}")
        };
        let _ = SearchRepository::new(conn)
            .upsert("session", session_id, profile_id, &title, &content, None);
    }
}

pub fn remove_session(conn: &Connection, session_id: i64) {
    let _ = SearchRepository::new(conn).remove("session", session_id);
}

/// evaluation 同步。
pub fn sync_evaluation(conn: &Connection, profile_id: i64, eval_id: i64) {
    if let Ok((title, note)) = conn.query_row(
        "SELECT title, COALESCE(note,'') FROM evaluations WHERE id=?1 AND profile_id=?2",
        params![eval_id, profile_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) {
        let note: String = note.chars().take(1000).collect();
        let content = if note.is_empty() {
            title.clone()
        } else {
            format!("{title} {note}")
        };
        let _ = SearchRepository::new(conn).upsert(
            "evaluation",
            eval_id,
            profile_id,
            &title,
            &content,
            None,
        );
    }
}

pub fn remove_evaluation(conn: &Connection, eval_id: i64) {
    let _ = SearchRepository::new(conn).remove("evaluation", eval_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_intelligence::ingestion::DOCUMENT_CHUNK_ENTITY;
    use crate::migrations;
    use crate::repository::document_ingestion::{
        ChunkSection, DocumentIngestionRepository, NewChunk, NewSection, SectionParent,
    };
    use crate::repository::learning_item::LearningItemRepository;
    use crate::repository::study_profile::StudyProfileRepository;
    use rusqlite::{params, Connection};

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        migrations::run_migrations(&conn).unwrap();
        conn
    }

    /// 真实写入一篇文档（一个 revision + 一个 chunk），返回 (source_id, revision_id, chunk_id)。
    fn seed_document(
        conn: &Connection,
        profile_id: i64,
        item_id: i64,
        text: &str,
    ) -> (i64, i64, i64) {
        conn.execute(
            "INSERT INTO learning_attachments
                (profile_id, learning_item_id, session_id, attachment_type,
                 file_name, relative_path, mime_type, caption)
             VALUES (?1, ?2, NULL, 'file', 'a.pdf', 'p/a.pdf', 'application/pdf', '')",
            params![profile_id, item_id],
        )
        .unwrap();
        let attachment_id = conn.last_insert_rowid();
        let repo = DocumentIngestionRepository::new(conn);
        let source_id = repo
            .create_source(profile_id, attachment_id, "a.pdf", None, None, "attachment")
            .unwrap();
        let revision_id = repo
            .create_revision(profile_id, source_id, None, Some("docling"), Some("2.73.0"))
            .unwrap();
        let sections = vec![NewSection {
            title: Some("Chapter 1".to_string()),
            ordinal: 0,
            parent: SectionParent::Root,
        }];
        let section_ids = repo
            .insert_sections(profile_id, revision_id, source_id, &sections)
            .unwrap();
        repo.insert_chunks(
            profile_id,
            revision_id,
            source_id,
            &[NewChunk {
                ordinal: 0,
                text: text.to_string(),
                section: ChunkSection::Local(0),
            }],
            &section_ids,
        )
        .unwrap();
        let chunk_id: i64 = conn
            .query_row(
                "SELECT id FROM document_chunks WHERE profile_id = ?1 AND revision_id = ?2",
                params![profile_id, revision_id],
                |r| r.get(0),
            )
            .unwrap();
        (source_id, revision_id, chunk_id)
    }

    // FP-1/FP-2 单元层：纯通配符 → 空集；保序去重；截断；上限；like_safe 转换。
    #[test]
    fn cjk_like_pipeline_unit() {
        assert!(cjk_like_terms("%_%").is_empty());
        assert!(cjk_like_terms("_ _ %").is_empty());
        assert_eq!(
            cjk_like_terms("光合作用 呼吸作用"),
            vec!["光合作用".to_string(), "呼吸作用".to_string()]
        );
        // 保序去重（case-sensitive，不去重不同大小写）
        assert_eq!(
            cjk_like_terms("a a a b"),
            vec!["a".to_string(), "b".to_string()]
        );
        // 截断到 MAX_CJK_FALLBACK_TERM_CHARS
        let long = "x".repeat(200);
        let t = cjk_like_terms(&long);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].chars().count(), MAX_CJK_FALLBACK_TERM_CHARS);
        // 上限 MAX_CJK_FALLBACK_TERMS
        let many = (0..50).map(|i| format!("t{i}")).collect::<Vec<_>>().join(" ");
        let t2 = cjk_like_terms(&many);
        assert_eq!(t2.len(), MAX_CJK_FALLBACK_TERMS);
        // like_safe 转换 LIKE 通配符与转义敏感字符
        assert_eq!(like_safe("a%b_c\\d"), "a b c d");
    }

    // OM-CJK-SAFE-01：query 含 % / _ 不得扩大成近似全表命中，
    // 不得越过 profile scope，不得越过 Ready revision scope。
    #[test]
    fn om_cjk_safe_01_wildcards_neutralized_and_scoped() {
        let conn = setup();
        let p1 = StudyProfileRepository::new(&conn)
            .create("p1", None, None, None, None, None)
            .unwrap()
            .id;
        let p2 = StudyProfileRepository::new(&conn)
            .create("p2", None, None, None, None, None)
            .unwrap()
            .id;
        let i1 = LearningItemRepository::new(&conn)
            .create_for_profile(p1, None, "i1", None, None)
            .unwrap()
            .id;
        let i2 = LearningItemRepository::new(&conn)
            .create_for_profile(p2, None, "i2", None, None)
            .unwrap()
            .id;

        // p1 授权 revision（内容含中文），p1 另有一份未授权的旧版，p2 一份外部档案。
        let (_s_ok, r_ok, c_ok) = seed_document(&conn, p1, i1, "光合作用 细胞 结构");
        let (_s_old, r_old, c_old) = seed_document(&conn, p1, i1, "光合作用 旧版本内容");
        let (_s2, r2, c2) = seed_document(&conn, p2, i2, "光合作用 外部档案专属");

        for (pid, cid, rev) in [(p1, c_ok, r_ok), (p1, c_old, r_old), (p2, c2, r2)] {
            SearchRepository::new(&conn)
                .upsert(
                    DOCUMENT_CHUNK_ENTITY,
                    cid,
                    pid,
                    "光合作用章节",
                    &format!("光合作用 rev{} body", rev),
                    None,
                )
                .unwrap();
        }

        // 纯通配符 query：必须 fail closed（空集），绝不全表命中 / 越权。
        let pure = SearchRepository::new(&conn)
            .search_scoped_by_revisions(p1, DOCUMENT_CHUNK_ENTITY, "%", &[r_ok], 50)
            .expect("pure-wildcard scoped search must not error");
        assert!(
            pure.is_empty(),
            "pure-wildcard query must fail closed, got {}",
            pure.len()
        );

        // 含通配符的 query：通配符被中和，且严格限定在 r_ok 授权 revision 内。
        let hits = SearchRepository::new(&conn)
            .search_scoped_by_revisions(p1, DOCUMENT_CHUNK_ENTITY, "_光合作用_%", &[r_ok], 50)
            .expect("scoped search must not error");
        // like_safe → " 光合作用 " → term = ["光合作用"] → 命中 r_ok 的 chunk（同 profile + 授权 revision）。
        assert_eq!(hits.len(), 1, "must match exactly the authorized revision chunk");
        assert_eq!(hits[0].entity_id, c_ok);
        // 未授权 revision（同 profile）/ 别的 profile 一律不出现。
        assert!(!hits.iter().any(|h| h.entity_id == c_old));
        assert!(!hits.iter().any(|h| h.entity_id == c2));
    }

    // OM-CJK-SAFE-02：普通中文「光合作用」在 wildcard 修复后仍然可以正常召回（不回归）。
    #[test]
    fn om_cjk_safe_02_normal_chinese_recall() {
        let conn = setup();
        let p1 = StudyProfileRepository::new(&conn)
            .create("p1", None, None, None, None, None)
            .unwrap()
            .id;
        let i1 = LearningItemRepository::new(&conn)
            .create_for_profile(p1, None, "i1", None, None)
            .unwrap()
            .id;
        let (_s, r_ok, c_ok) =
            seed_document(&conn, p1, i1, "光合作用 是植物利用光能合成有机物的过程");
        SearchRepository::new(&conn)
            .upsert(
                DOCUMENT_CHUNK_ENTITY,
                c_ok,
                p1,
                "光合作用章节",
                "光合作用 是植物利用光能合成有机物的过程",
                None,
            )
            .unwrap();

        // 非 scoped 通用检索：FTS(unicode61 中文分 token 差) 走 CJK 回退，必须召回。
        let via_search = SearchRepository::new(&conn)
            .search(p1, "光合作用", None, 50)
            .expect("search must not error");
        assert!(
            via_search.iter().any(|h| h.entity_id == c_ok),
            "normal Chinese '光合作用' must still be recalled via search()"
        );

        // scoped 检索同样召回。
        let via_scoped = SearchRepository::new(&conn)
            .search_scoped_by_revisions(p1, DOCUMENT_CHUNK_ENTITY, "光合作用", &[r_ok], 50)
            .expect("scoped search must not error");
        assert!(
            via_scoped.iter().any(|h| h.entity_id == c_ok),
            "normal Chinese '光合作用' must still be recalled via scoped search"
        );
    }
}
