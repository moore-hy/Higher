//! 私人化资料（DEV-0052 / PHASE G-H §56-81, PHASE I 模板, PHASE J 维护）。
//!
//! 导入：txt/md（UTF-8/BOM/GBK/GB18030）、docx（ZIP+XML 解析）、pdf（文本流提取）。
//! RAM-light：顺序读取分块（≤256KB），中间结果落盘/SQLite。
//! Compile：Map（每 source 结构化要点）→ Merge（冲突不擅自取舍）→ 19 节固定结构 MD。
//! 注：zip/scraper/pdf-extract 依赖链含 build-script crate 被 Windows SAC 拦截，
//! 改用 flate2(rust_backend，已在 png 依赖树中) + 手写解析（见 TRAE_RUN）。

use rusqlite::{params, Connection};
use std::io::Read as _;
use std::path::Path;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PersonalizationSource {
    pub id: i64,
    pub profile_id: i64,
    pub file_name: String,
    pub file_type: String,
    pub relative_path: String,
    pub sha256: String,
    pub extracted_text_path: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// DEV-0059 §8：PersonalProfile version rows（v021 重建后结构）。
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct PersonalizationProfile {
    pub id: i64,
    pub profile_id: i64,
    pub version: i64,
    pub md_content: String,
    pub structured_json: Option<String>,
    pub status: String,
    pub based_on_version_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub confirmed_at: Option<String>,
}

pub struct PersonalizationRepository<'a> {
    conn: &'a Connection,
}

impl<'a> PersonalizationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_source(
        &self,
        profile_id: i64,
        file_name: &str,
        file_type: &str,
        relative_path: &str,
        sha256: &str,
        extracted_text_path: &str,
        status: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO personalization_sources
                 (profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_source_status(&self, id: i64, status: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE personalization_sources SET status=?1, updated_at=datetime('now') WHERE id=?2",
                params![status, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_sources(&self, profile_id: i64) -> Result<Vec<PersonalizationSource>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status, created_at, updated_at
                      FROM personalization_sources WHERE profile_id=?1 ORDER BY id DESC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], parse_src)
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    pub fn get_source(&self, id: i64, profile_id: i64) -> Result<Option<PersonalizationSource>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, profile_id, file_name, file_type, relative_path, sha256, extracted_text_path, status, created_at, updated_at
                      FROM personalization_sources WHERE id=?1 AND profile_id=?2")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![id, profile_id], parse_src).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    pub fn delete_source(&self, id: i64, profile_id: i64) -> Result<(), String> {
        self.get_source(id, profile_id)?.ok_or("资料不存在或不属于当前档案")?;
        self.conn
            .execute("DELETE FROM personalization_source_chunks WHERE source_id=?1", params![id])
            .map_err(|e| e.to_string())?;
        self.conn
            .execute("DELETE FROM personalization_sources WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 分块写入（§67-68：chunk ≤ 256KB 文本）+ FTS。
    pub fn store_chunks(&self, source_id: i64, profile_id: i64, text: &str) -> Result<usize, String> {
        self.conn
            .execute("DELETE FROM personalization_source_chunks WHERE source_id=?1", params![source_id])
            .map_err(|e| e.to_string())?;
        let cap = 256 * 1024;
        let mut idx = 0i64;
        let mut n = 0usize;
        for chunk in chunk_text(text, cap) {
            self.conn
                .execute(
                    "INSERT INTO personalization_source_chunks (source_id, profile_id, chunk_index, content)
                     VALUES (?1,?2,?3,?4)",
                    params![source_id, profile_id, idx, chunk],
                )
                .map_err(|e| e.to_string())?;
            let chunk_row_id = self.conn.last_insert_rowid();
            let _ = crate::repository::search::SearchRepository::new(self.conn).upsert(
                "personalization_chunk",
                chunk_row_id,
                profile_id,
                &format!("chunk-{}", idx),
                chunk.as_str(),
                None,
            );
            idx += 1;
            n += 1;
        }
        Ok(n)
    }

    pub fn all_chunks(&self, profile_id: i64) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT source_id, content FROM personalization_source_chunks WHERE profile_id=?1 ORDER BY source_id, chunk_index")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![profile_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    // ---- Profile（DEV-0059 §8：version rows） ----

    /// 兼容取法：confirmed 优先，否则最新 draft（旧调用方语义）。
    pub fn get_profile(&self, profile_id: i64) -> Result<Option<PersonalizationProfile>, String> {
        if let Some(p) = self.get_confirmed_profile(profile_id)? {
            return Ok(Some(p));
        }
        self.get_draft_profile(profile_id)
    }

    /// 当前正式（confirmed）版本。
    pub fn get_confirmed_profile(&self, profile_id: i64) -> Result<Option<PersonalizationProfile>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, version, md_content, structured_json, status, based_on_version_id, created_at, updated_at, confirmed_at
                 FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![profile_id], parse_profile).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 当前 draft 版本（每 profile 最多 1 条，v021 partial unique 约束）。
    pub fn get_draft_profile(&self, profile_id: i64) -> Result<Option<PersonalizationProfile>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, version, md_content, structured_json, status, based_on_version_id, created_at, updated_at, confirmed_at
                 FROM personalization_profiles WHERE profile_id=?1 AND status='draft' ORDER BY version DESC LIMIT 1",
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query_map(params![profile_id], parse_profile).map_err(|e| e.to_string())?;
        rows.next().transpose().map_err(|e| e.to_string())
    }

    /// 全部版本（含 superseded；历史可查）。
    pub fn list_profile_versions(&self, profile_id: i64) -> Result<Vec<PersonalizationProfile>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, profile_id, version, md_content, structured_json, status, based_on_version_id, created_at, updated_at, confirmed_at
                 FROM personalization_profiles WHERE profile_id=?1 ORDER BY version DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![profile_id], parse_profile).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    /// §8/§10.1：保存 Draft——已有 draft 则原地更新；否则新建 vN+1
    /// （based_on_version_id = 当前 confirmed id；旧 confirmed 不动）。
    pub fn save_draft(&self, profile_id: i64, md: &str, structured: Option<&str>) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let existing_draft: Option<i64> = tx
            .query_row(
                "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='draft'",
                params![profile_id],
                |r| r.get(0),
            )
            .ok();
        let confirmed_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .ok();
        match existing_draft {
            Some(did) => {
                tx.execute(
                    "UPDATE personalization_profiles SET md_content=?1, structured_json=?2, updated_at=datetime('now')
                     WHERE id=?3",
                    params![md, structured, did],
                )
                .map_err(|e| e.to_string())?;
            }
            None => {
                let next_ver: i64 = tx
                    .query_row(
                        "SELECT COALESCE(MAX(version),0)+1 FROM personalization_profiles WHERE profile_id=?1",
                        params![profile_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO personalization_profiles (profile_id, version, md_content, structured_json, status, based_on_version_id)
                     VALUES (?1,?2,?3,?4,'draft',?5)",
                    params![profile_id, next_ver, md, structured, confirmed_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// §8/§25.2：Draft 确认 → 正式（一个 transaction）：
    /// old confirmed → superseded；new draft → confirmed + confirmed_at。
    pub fn confirm(&self, profile_id: i64) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let draft_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='draft' ORDER BY version DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .ok();
        let Some(did) = draft_id else {
            return Err("还没有待确认的草稿".to_string());
        };
        tx.execute(
            "UPDATE personalization_profiles SET status='superseded', updated_at=datetime('now')
             WHERE profile_id=?1 AND status='confirmed'",
            params![profile_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE personalization_profiles SET status='confirmed', confirmed_at=datetime('now'), updated_at=datetime('now')
             WHERE id=?1",
            params![did],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    /// DEV-0059.1 §6：Draft 落库并写入「版本-来源」snapshot relation
    /// （personalization_profile_sources）。Confirm 后 relation 保持；新 Source
    /// 后重新 Compile 产生 vN+1 的新 snapshot，不改变 vN 的历史 snapshot。
    /// source_ids 为空时仅更新 Draft 内容，保持原 snapshot。
    pub fn save_draft_with_sources(
        &self,
        profile_id: i64,
        md: &str,
        structured: Option<&str>,
        source_ids: &[i64],
    ) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let existing_draft: Option<i64> = tx
            .query_row(
                "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='draft'",
                params![profile_id],
                |r| r.get(0),
            )
            .ok();
        let confirmed_id: Option<i64> = tx
            .query_row(
                "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='confirmed' ORDER BY version DESC LIMIT 1",
                params![profile_id],
                |r| r.get(0),
            )
            .ok();
        let draft_id: i64 = match existing_draft {
            Some(did) => {
                tx.execute(
                    "UPDATE personalization_profiles SET md_content=?1, structured_json=?2, updated_at=datetime('now')
                     WHERE id=?3",
                    params![md, structured, did],
                )
                .map_err(|e| e.to_string())?;
                did
            }
            None => {
                let next_ver: i64 = tx
                    .query_row(
                        "SELECT COALESCE(MAX(version),0)+1 FROM personalization_profiles WHERE profile_id=?1",
                        params![profile_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO personalization_profiles (profile_id, version, md_content, structured_json, status, based_on_version_id)
                     VALUES (?1,?2,?3,?4,'draft',?5)",
                    params![profile_id, next_ver, md, structured, confirmed_id],
                )
                .map_err(|e| e.to_string())?;
                tx.last_insert_rowid()
            }
        };
        if !source_ids.is_empty() {
            tx.execute(
                "DELETE FROM personalization_profile_sources WHERE profile_version_id=?1",
                params![draft_id],
            )
            .map_err(|e| e.to_string())?;
            for sid in source_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO personalization_profile_sources (profile_version_id, source_id) VALUES (?1,?2)",
                    params![draft_id, sid],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// DEV-0059.1 §6：某 PersonalProfile 版本使用的 Source snapshot（只读）。
    pub fn list_sources_for_version(
        &self,
        version_id: i64,
        profile_id: i64,
    ) -> Result<Vec<PersonalizationSource>, String> {
        let owned: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM personalization_profiles WHERE id=?1 AND profile_id=?2",
                params![version_id, profile_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if owned == 0 {
            return Err("版本不存在或不属于当前档案".to_string());
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, s.profile_id, s.file_name, s.file_type, s.relative_path, s.sha256, s.extracted_text_path, s.status, s.created_at, s.updated_at
                 FROM personalization_sources s
                 JOIN personalization_profile_sources ps ON ps.source_id = s.id
                 WHERE ps.profile_version_id=?1 ORDER BY s.id",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![version_id], parse_src).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }
    pub fn user_edit(&self, profile_id: i64, md: &str) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;
        let _ = user_edit_in_tx(&tx, profile_id, md)?;
        tx.commit().map_err(|e| e.to_string())?;
        // DEV-0076 §十二：用户亲手编辑 = 用户事实（非 AI 推断）→ 直接 confirmed
        let _ = self.conn.execute(
            "INSERT INTO memory_records (profile_id, memory_type, category, memory_key, memory_value, source_kind, source_excerpt, importance, confidence, status)
             VALUES (?1,'user_fact','personalization','私人化档案（用户编辑）',?2,'user_edit',?3,5,'high','confirmed')",
            params![profile_id, md, md],
        );
        Ok(())
    }

    /// §10.1/§41：新信息提示（dirty 列已在 v021 移除）——若 confirmed 存在且无 draft，
    /// 创建基于 confirmed 的 draft vN+1（UI 显示"有更新，建议重新整合"）。
    pub fn mark_dirty(&self, profile_id: i64) -> Result<(), String> {
        if self.get_confirmed_profile(profile_id)?.is_none() {
            return Ok(());
        }
        if self.get_draft_profile(profile_id)?.is_some() {
            return Ok(());
        }
        let confirmed = self.get_confirmed_profile(profile_id)?.unwrap();
        let next_ver = confirmed.version + 1;
        self.conn
            .execute(
                "INSERT INTO personalization_profiles (profile_id, version, md_content, structured_json, status, based_on_version_id)
                 VALUES (?1,?2,?3,?4,'draft',?5)",
                params![profile_id, next_ver, confirmed.md_content, confirmed.structured_json, confirmed.id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// §10.1/§25.2 用户编辑核心（供 ChangeSet apply 外层事务内复用；不嵌套新事务）。
pub fn user_edit_in_tx(
    tx: &rusqlite::Transaction<'_>,
    profile_id: i64,
    md: &str,
) -> Result<(), String> {
    tx.execute(
        "UPDATE personalization_profiles SET status='superseded', updated_at=datetime('now')
         WHERE profile_id=?1 AND status='confirmed'",
        params![profile_id],
    )
    .map_err(|e| e.to_string())?;
    let next_ver: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM personalization_profiles WHERE profile_id=?1",
            params![profile_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let confirmed_id: Option<i64> = tx
        .query_row(
            "SELECT id FROM personalization_profiles WHERE profile_id=?1 AND status='superseded' ORDER BY version DESC LIMIT 1",
            params![profile_id],
            |r| r.get(0),
        )
        .ok();
    tx.execute(
        "INSERT INTO personalization_profiles (profile_id, version, md_content, status, based_on_version_id, confirmed_at)
         VALUES (?1,?2,?3,'confirmed',?4,datetime('now'))",
        params![profile_id, next_ver, md, confirmed_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_src(r: &rusqlite::Row<'_>) -> rusqlite::Result<PersonalizationSource> {
    Ok(PersonalizationSource {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        file_name: r.get(2)?,
        file_type: r.get(3)?,
        relative_path: r.get(4)?,
        sha256: r.get(5)?,
        extracted_text_path: r.get(6)?,
        status: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn parse_profile(r: &rusqlite::Row<'_>) -> rusqlite::Result<PersonalizationProfile> {
    Ok(PersonalizationProfile {
        id: r.get(0)?,
        profile_id: r.get(1)?,
        version: r.get(2)?,
        md_content: r.get(3)?,
        structured_json: r.get(4)?,
        status: r.get(5)?,
        based_on_version_id: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        confirmed_at: r.get(9)?,
    })
}

fn chunk_text(text: &str, cap: usize) -> Vec<String> {
    chunk_text_pub(text, cap)
}

/// 分块（DEV-0059 §13：planning_source 复用同一分块语义）。
pub fn chunk_text_pub(text: &str, cap: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split("\n\n") {
        if !cur.is_empty() && cur.len() + para.len() + 2 > cap {
            out.push(std::mem::take(&mut cur));
        }
        if para.len() > cap {
            let mut rest = para;
            while rest.len() > cap {
                let mut cut = cap;
                while cut > 0 && !rest.is_char_boundary(cut) {
                    cut -= 1;
                }
                let (head, tail) = rest.split_at(cut);
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                out.push(head.to_string());
                rest = tail;
            }
            if !cur.is_empty() {
                cur.push_str("\n\n");
            }
            cur.push_str(rest);
        } else {
            if !cur.is_empty() {
                cur.push_str("\n\n");
            }
            cur.push_str(para);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// =============== 文本提取 ===============

/// §62：UTF-8 / UTF-8 BOM / GBK / GB18030 fallback；失败明确报错。
pub fn decode_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec())
            .map_err(|_| "文件不是有效的 UTF-8 文本".to_string());
    }
    if let Ok(s) = String::from_utf8(bytes.clone()) {
        return Ok(s);
    }
    let (decoded, _, had_errors) = encoding_rs::GB18030.decode(&bytes);
    if had_errors {
        return Err("无法解码文件（既不是 UTF-8 也不是 GBK/GB18030）".to_string());
    }
    Ok(decoded.into_owned())
}

fn inflate_raw(data: &[u8]) -> Result<Vec<u8>, ()> {
    miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| ())
}

fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>, ()> {
    miniz_oxide::inflate::decompress_to_vec_zlib(data).map_err(|_| ())
}

/// §63 DOCX：ZIP 容器 + word/document.xml（w:t 拼接）。
pub fn extract_docx(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开 DOCX 失败：{e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| format!("读取 DOCX 失败：{e}"))?;
    if buf.len() < 4 || &buf[..2] != b"PK" {
        return Err("这不是有效的 .docx 文件（旧版 .doc 请先转换为 .docx / .pdf / .txt）".to_string());
    }
    let mut pos = 0usize;
    while pos + 30 <= buf.len() {
        if &buf[pos..pos + 4] != b"PK\x03\x04" {
            break;
        }
        let method = u16::from_le_bytes([buf[pos + 8], buf[pos + 9]]);
        let csize = u32::from_le_bytes([buf[pos + 18], buf[pos + 19], buf[pos + 20], buf[pos + 21]]) as usize;
        let nlen = u16::from_le_bytes([buf[pos + 26], buf[pos + 27]]) as usize;
        let elen = u16::from_le_bytes([buf[pos + 28], buf[pos + 29]]) as usize;
        let name_start = pos + 30;
        let data_start = name_start + nlen + elen;
        if name_start + nlen > buf.len() || data_start + csize > buf.len() {
            break;
        }
        let name = String::from_utf8_lossy(&buf[name_start..name_start + nlen]).to_string();
        let raw = &buf[data_start..data_start + csize];
        if name == "word/document.xml" {
            let xml_bytes = match method {
                0 => raw.to_vec(),
                8 => inflate_raw(raw).map_err(|_| "DOCX 内部数据解压失败（文件可能已损坏）".to_string())?,
                m => return Err(format!("DOCX 使用了不支持的压缩方式（{m}）")),
            };
            let xml = String::from_utf8_lossy(&xml_bytes).to_string();
            return Ok(extract_wt_text(&xml));
        }
        if csize == 0 {
            pos = data_start;
        } else {
            pos = data_start + csize;
        }
    }
    Err("DOCX 中未找到 word/document.xml（文件可能不是标准 Word 文档）".to_string())
}

/// 从 document.xml 提取 w:t 文本（w:p 结尾换行；XML 实体解码）。
fn extract_wt_text(xml: &str) -> String {
    let mut out = String::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;
    let mut tag = String::new();
    let mut in_tag = false;
    let mut in_wt = false;
    let mut text_buf = String::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !in_tag && c == '<' {
            if in_wt {
                out.push_str(&xml_entities(&std::mem::take(&mut text_buf)));
                in_wt = false;
            }
            tag.clear();
            in_tag = true;
            i += 1;
            continue;
        }
        if in_tag {
            if c == '>' {
                let t = tag.trim().to_string();
                if t.starts_with("w:t") && !t.starts_with("/w:t") {
                    in_wt = true;
                } else if t.starts_with("/w:t") && in_wt {
                    out.push_str(&xml_entities(&std::mem::take(&mut text_buf)));
                    in_wt = false;
                } else if t.starts_with("/w:p") {
                    out.push('\n');
                }
                in_tag = false;
                i += 1;
                continue;
            }
            tag.push(c);
            i += 1;
            continue;
        }
        if in_wt {
            let l = utf8_len(bytes[i]);
            let e = (i + l).min(bytes.len());
            text_buf.push_str(&String::from_utf8_lossy(&bytes[i..e]));
            i = e;
            continue;
        }
        i += 1;
    }
    if in_wt {
        out.push_str(&xml_entities(&text_buf));
    }
    out.trim().to_string()
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

fn xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// §64 PDF：文本型提取（stream FlateDecode/未压缩 + Tj/TJ 字符串）。
/// 扫描型（提取为空/极少）→ 明确报错（V1 无 OCR，AI 不得假装读过）。
pub fn extract_pdf(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开 PDF 失败：{e}"))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).map_err(|e| format!("读取 PDF 失败：{e}"))?;
    if !buf.starts_with(b"%PDF") {
        return Err("这不是有效的 PDF 文件".to_string());
    }
    let mut all_text = String::new();
    let mut pos = 0usize;
    while let Some(rel) = find_sub(&buf, b"stream", pos) {
        let mut start = rel + 6;
        if start < buf.len() && buf[start] == b'\r' {
            start += 2;
        } else if start < buf.len() && buf[start] == b'\n' {
            start += 1;
        }
        let Some(end_rel) = find_sub(&buf, b"endstream", start) else { break };
        let data = &buf[start..end_rel];
        let obj_start = find_sub_rev(&buf[..rel], b"<<").map(|x| x + 2).unwrap_or(0);
        let dict = String::from_utf8_lossy(&buf[obj_start..rel]).to_string();
        let decoded: Vec<u8> = if dict.contains("FlateDecode") {
            inflate_zlib(data).unwrap_or_default()
        } else {
            data.to_vec()
        };
        let content = String::from_utf8_lossy(&decoded);
        all_text.push_str(&extract_pdf_text_ops(&content));
        pos = end_rel + 9;
    }
    let clean = all_text.trim().to_string();
    if clean.chars().count() < 30 {
        return Err(
            "该 PDF 未检测到足够可提取文字，目前 Higher V1 不包含 OCR。请提供文本型 PDF 或转换后导入。".to_string(),
        );
    }
    Ok(clean)
}

fn find_sub_rev(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

fn find_sub(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// 从 content stream 提取 (...)Tj 与 [ ... ]TJ 文本（八进制/转义/多字节）。
fn extract_pdf_text_ops(content: &str) -> String {
    let mut out = String::new();
    let b = content.as_bytes();
    let mut i = 0usize;
    let mut collected = false;
    while i < b.len() {
        match b[i] {
            b'(' => {
                let mut depth = 1;
                let mut j = i + 1;
                let mut s = String::new();
                while j < b.len() && depth > 0 {
                    match b[j] {
                        b'\\' => {
                            if j + 1 < b.len() {
                                match b[j + 1] {
                                    b'n' => s.push('\n'),
                                    b'r' => s.push('\r'),
                                    b't' => s.push('\t'),
                                    b'(' => s.push('('),
                                    b')' => s.push(')'),
                                    b'\\' => s.push('\\'),
                                    b'0'..=b'7' => {
                                        let mut val = 0u32;
                                        let mut k = j + 1;
                                        let mut cnt = 0;
                                        while k < b.len() && cnt < 3 && (b'0'..=b'7').contains(&b[k]) {
                                            val = val * 8 + (b[k] - b'0') as u32;
                                            k += 1;
                                            cnt += 1;
                                        }
                                        if let Some(ch) = char::from_u32(val) {
                                            s.push(ch);
                                        }
                                        j = k - 1;
                                    }
                                    _ => {}
                                }
                                j += 2;
                                continue;
                            }
                        }
                        b'(' => {
                            depth += 1;
                            s.push('(');
                        }
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            s.push(')');
                        }
                        _ => {
                            let l = utf8_len(b[j]);
                            let e = (j + l).min(b.len());
                            s.push_str(&String::from_utf8_lossy(&b[j..e]));
                            j += l;
                            continue;
                        }
                    }
                    j += 1;
                }
                out.push_str(&s);
                collected = true;
                i = j + 1;
            }
            b'T' | b'E' => {
                if collected {
                    if b[i..].starts_with(b"Td") || b[i..].starts_with(b"TD") || b[i..].starts_with(b"T*") {
                        out.push('\n');
                        collected = false;
                        i += 2;
                        continue;
                    }
                    if b[i..].starts_with(b"ET") {
                        out.push('\n');
                        collected = false;
                        i += 2;
                        continue;
                    }
                    if b[i..].starts_with(b"Tj") || b[i..].starts_with(b"TJ") {
                        i += 2;
                        continue;
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// §82-85：AI 需求采集模板（静态导出）。
pub const REQUIREMENT_TEMPLATE_MD: &str = "# Higher AI 需求采集模板\n\n你是一位专业的长期学习顾问。请通过对用户的多轮访谈，产出一份《Higher 私人化学习档案》。\n\n## 访谈规则（必须遵守）\n- 不要一次问所有问题；每次最多问 2~3 个相关问题，根据回答继续追问。\n- 用户可以随时主动补充信息。\n- 不确定的信息不得编造；区分：事实（用户明确说的）/ 用户观点（主观判断）/ AI 推断（你的分析）。\n- 发现前后矛盾时，明确指出并请用户确认。\n- 最终输出一个 Markdown 文档，包含下方全部章节；无法确认的内容放入「尚未确认 / 冲突信息」。\n\n## 访谈重点（按需覆盖）\n个人基本情况 / 学历 / 专业 / 当前状态 / 长期目标 / 目标时间 / 当前水平 / 优势 / 短板 / 失败经历 / 成功经历 / 工作日可学习时间 / 周末时间 / 作息 / 喜欢的学习方式 / 不喜欢的学习方式 / 拖延情况 / 学习环境 / 已有资料 / 目前进度 / 重要限制 / 用户主动要求\n\n## 输出结构\n# Higher 私人化学习档案\n## 1. 基本情况\n## 2. 学历与专业背景\n## 3. 当前状态\n## 4. 最终学习目标\n## 5. 当前能力基础\n## 6. 优势\n## 7. 明显短板\n## 8. 学习习惯\n## 9. 时间条件\n## 10. 学习偏好\n## 11. 既往学习经历\n## 12. 当前学习进度\n## 13. 重要限制条件\n## 14. 用户明确要求\n## 15. Higher 客观观察（访谈中留空，导入 Higher 后由系统补充）\n## 16. AI 推断（格式：内容 + 依据 + 置信度（低/中/高） + 生成时间）\n## 17. 尚未确认 / 冲突信息\n## 18. 资料来源\n## 19. 更新历史\n";

/// §73：19 节固定结构骨架。
pub fn profile_md_skeleton() -> String {
    format!(
        "# Higher 私人化学习档案\n\n{}\n\n## 19. 更新历史\n- 首次生成：待 Compile\n",
        (1..=18)
            .map(|i| format!("## {}. {}\n（待补充）", i, section_title(i)))
            .collect::<Vec<_>>()
            .join("\n\n")
    )
}

pub fn section_title(i: usize) -> &'static str {
    match i {
        1 => "基本情况",
        2 => "学历与专业背景",
        3 => "当前状态",
        4 => "最终学习目标",
        5 => "当前能力基础",
        6 => "优势",
        7 => "明显短板",
        8 => "学习习惯",
        9 => "时间条件",
        10 => "学习偏好",
        11 => "既往学习经历",
        12 => "当前学习进度",
        13 => "重要限制条件",
        14 => "用户明确要求",
        15 => "Higher 客观观察",
        16 => "AI 推断",
        17 => "尚未确认 / 冲突信息",
        18 => "资料来源",
        _ => "更新历史",
    }
}

/// §73 Compile 用：1-14 节序（15-19 由系统/AI 单独生成）。
pub fn section_title_seq() -> Vec<&'static str> {
    (1..=14).map(section_title).collect()
}

/// DEV-0059.1 §7：structured_json contract 的 schema_version。
pub const PERSONAL_STRUCTURED_SCHEMA_VERSION: i64 = 1;

/// DEV-0059.1 §7：raw `facts[]` → structured_json contract（schema_version:1）。
///
/// 字段：
/// - basics（basic_info + goal）/ capabilities / strengths / weaknesses / habits /
///   preferences（含 requirements）/ constraints / availability（time_conditions）/
///   current_state（state + progress）/ unresolved
/// - field_provenance：每个 bucket 的资料来源（section → source 列表）
///
/// 无法可靠归类的条目进 unresolved；冲突/待确认进 unresolved（extra）；禁止猜值。
/// Markdown（md_content）仍保留完整人类可读档案；本函数只产出机器输入。
pub fn build_personal_structured(facts: &[serde_json::Value], unresolved_extra: &[String]) -> String {
    let mut basic_info: Vec<serde_json::Value> = Vec::new();
    let mut capabilities: Vec<serde_json::Value> = Vec::new();
    let mut strengths: Vec<serde_json::Value> = Vec::new();
    let mut weaknesses: Vec<serde_json::Value> = Vec::new();
    let mut habits: Vec<serde_json::Value> = Vec::new();
    let mut preferences: Vec<serde_json::Value> = Vec::new();
    let mut constraints: Vec<serde_json::Value> = Vec::new();
    let mut time_conditions: Vec<serde_json::Value> = Vec::new();
    let mut state: Vec<serde_json::Value> = Vec::new();
    let mut progress: Vec<serde_json::Value> = Vec::new();
    let mut unresolved: Vec<serde_json::Value> = Vec::new();
    let mut provenance: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();

    for f in facts {
        let section = f.get("section").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let text = f.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let kind = f.get("kind").and_then(|x| x.as_str()).unwrap_or("fact").to_string();
        let source = f.get("source").and_then(|x| x.as_str()).unwrap_or("?").to_string();
        if text.is_empty() {
            continue;
        }
        let item = serde_json::json!({ "text": text, "kind": kind, "source": source.clone() });
        provenance.entry(section.clone()).or_default().push(source.clone());
        match section.as_str() {
            "基本情况" => basic_info.push(item),
            // DEV-0059.2 §4：个人资料中的目标描述只能作为 source observation / candidate，
            // 不得成为 PersonalProfile 的正式 Goal（GoalTarget 才是 Canonical）
            "最终学习目标" => unresolved.push(serde_json::json!({
                "text": text,
                "kind": "goal_observation",
                "source": source,
                "note": "个人资料中的目标描述，仅供 GoalTarget 确认参考，不是正式目标",
            })),
            "学历与专业背景" | "当前能力基础" | "既往学习经历" => capabilities.push(item),
            "优势" => strengths.push(item),
            "明显短板" => weaknesses.push(item),
            "学习习惯" => habits.push(item),
            "学习偏好" | "用户明确要求" => preferences.push(item),
            "重要限制条件" => constraints.push(item),
            "时间条件" => time_conditions.push(item),
            "当前状态" => state.push(item),
            "当前学习进度" => progress.push(item),
            _ => unresolved.push(serde_json::json!({
                "text": text, "kind": kind, "source": source.clone(), "note": "无法可靠归类的资料条目"
            })),
        }
    }
    for u in unresolved_extra {
        unresolved.push(serde_json::json!({ "text": u, "kind": "conflict", "source": "compile", "note": "冲突/待确认" }));
    }
    serde_json::json!({
        "schema_version": PERSONAL_STRUCTURED_SCHEMA_VERSION,
        "basics": { "basic_info": basic_info },
        "capabilities": capabilities,
        "strengths": strengths,
        "weaknesses": weaknesses,
        "habits": habits,
        "preferences": preferences,
        "constraints": constraints,
        "availability": { "time_conditions": time_conditions },
        "current_state": { "state": state, "progress": progress },
        "unresolved": unresolved,
        "field_provenance": serde_json::to_value(provenance).unwrap_or(serde_json::json!({})),
    })
    .to_string()
}
