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
        let match_expr = match_words.join(" OR ");
        let type_filter = match entity_types {
            Some(ts) if !ts.is_empty() => {
                let quoted: Vec<String> = ts.iter().map(|t| format!("'{}'", t.replace('\'', ""))).collect();
                format!(" AND si.entity_type IN ({})", quoted.join(","))
            }
            _ => String::new(),
        };
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
        if !out.is_empty() {
            return Ok(out);
        }
        // CJK fallback：unicode61 对中文按整段分 token（子串不可命中）→ LIKE 子串匹配。
        // 仍走 search_index 表（磁盘查询 + profile 隔离 + 类型过滤），非全表内存扫描。
        let like = format!("%{}%", q.replace('%', " ").replace('_', " "));
        let sql2 = format!(
            "SELECT entity_type, entity_id, title, timestamp FROM search_index
             WHERE profile_id = ?1 AND (title LIKE ?2 OR content LIKE ?2){}
             ORDER BY entity_id DESC LIMIT ?3",
            type_filter
        );
        let mut stmt2 = self.conn.prepare(&sql2).map_err(|e| e.to_string())?;
        let rows2 = stmt2
            .query_map(params![profile_id, like, limit.clamp(1, 100)], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
            })
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
        Ok(out)
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
            let sql =
                "SELECT si.entity_id FROM search_fts fts
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
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, memory_type, importance, confidence, created_at, status, source_kind
                 FROM memory_records
                 WHERE profile_id = ?1 AND status = 'active'
                   AND (?2 = '' OR memory_key LIKE '%' || ?2 || '%' OR memory_value LIKE '%' || ?2 || '%')",
            )
            .map_err(|e| e.to_string())?;
        let like = q.replace(['%', '_'], " ");
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
                     WHERE id = ?1 AND profile_id = ?2 AND status = 'active'",
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
                s += match confidence.as_str() { "high" => 3.0, "medium" => 1.5, _ => 0.5 };
                // 类型优先级（§35）：user explicit ≈ higher_db > opinion > inference
                s += match mtype.as_str() {
                    "user_fact" | "user_constraint" | "user_preference" | "goal_context" => 3.0,
                    "system_observation" => 3.0,
                    "user_opinion" => 1.5,
                    _ => 0.5, // ai_inference
                };
                if is_user { s += 0.5; }
                // recency：每天衰减 0.02，下限 0.4
                let age_days = age_days_of(&created, now);
                s *= (1.0 - (age_days as f64) * 0.02).max(0.4);
                (s, id)
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(limit.clamp(1, 20) as usize).map(|(_, id)| id).collect())
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
