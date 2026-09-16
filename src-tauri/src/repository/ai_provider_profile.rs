//! ai_provider_profiles 持久化（DEV-0062 §10/§21/§22/§38）。
//!
//! - Connection = 一套确定 Provider + Model Config（display_name 唯一）
//! - 修改能力相关字段（adapter/base_url/api_key/model/thinking_mode）→ compatibility 重置 untested
//! - active 引用存 settings KV（ai.active_primary_profile_id / ai.active_control_profile_id）

use rusqlite::{params, Connection};

use crate::ai::provider::{AdapterKind, AiCapabilities, ThinkingMode};

pub const KEY_ACTIVE_PRIMARY: &str = "ai.active_primary_profile_id";
pub const KEY_ACTIVE_CONTROL: &str = "ai.active_control_profile_id";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiProviderProfile {
    pub id: i64,
    pub display_name: String,
    pub adapter_kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub thinking_mode: String,
    /// POST-M7 §S2-A：显式认证模式（"bearer" | "none"；v035 存量默认 bearer）。
    pub auth_mode: String,
    /// POST-M7 §S3：稳定凭据引用（真实 Key 存 OS SecretStore；NULL = 尚未迁移/无凭据）。
    pub secret_ref: Option<String>,
    pub enabled: bool,
    pub capabilities: AiCapabilities,
    pub compatibility_status: String,
    pub last_test_message: String,
    pub last_tested_at: Option<String>,
}

fn parse_row(row: &rusqlite::Row) -> rusqlite::Result<AiProviderProfile> {
    let caps_json: String = row.get(9)?;
    Ok(AiProviderProfile {
        id: row.get(0)?,
        display_name: row.get(1)?,
        adapter_kind: row.get(2)?,
        base_url: row.get(3)?,
        api_key: row.get(4)?,
        model: row.get(5)?,
        thinking_mode: row.get(6)?,
        auth_mode: row.get(12)?,
        secret_ref: row.get(13)?,
        enabled: row.get::<_, i64>(7)? == 1,
        capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
        compatibility_status: row.get(8)?,
        last_test_message: row.get(10)?,
        last_tested_at: row.get(11)?,
    })
}

const COLS: &str = "id, display_name, adapter_kind, base_url, api_key, model, thinking_mode, \
                    enabled, compatibility_status, capabilities_json, last_test_message, last_tested_at, auth_mode, secret_ref";

pub struct AiProviderProfileRepository<'a> {
    conn: &'a Connection,
}

impl<'a> AiProviderProfileRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self) -> rusqlite::Result<Vec<AiProviderProfile>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM ai_provider_profiles ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map([], parse_row)?;
        rows.collect()
    }

    pub fn list_enabled(&self) -> rusqlite::Result<Vec<AiProviderProfile>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM ai_provider_profiles WHERE enabled = 1 ORDER BY id ASC"
        ))?;
        let rows = stmt.query_map([], parse_row)?;
        rows.collect()
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<AiProviderProfile>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM ai_provider_profiles WHERE id = ?1"
        ))?;
        let mut rows = stmt.query_map(params![id], parse_row)?;
        Ok(rows.next().transpose()?)
    }

    /// 新建 Connection（compatibility 从 untested 开始；§36 Key 明文本地保存）。
    /// POST-M7 §S2：auth_mode 显式传入（"bearer" | "none"），不按 base_url 推断。
    pub fn create(
        &self,
        display_name: &str,
        adapter_kind: &AdapterKind,
        base_url: &str,
        api_key: &str,
        model: &str,
        thinking_mode: &ThinkingMode,
        auth_mode: &str,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO ai_provider_profiles
             (display_name, adapter_kind, base_url, api_key, model, thinking_mode, auth_mode)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                display_name.trim(),
                adapter_kind.as_str(),
                base_url.trim(),
                api_key.trim(),
                model.trim(),
                thinking_mode.as_str(),
                auth_mode
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// POST-M7 hotfix P0-2：Bearer 新建 DB-atomic——**单语句 INSERT** 同时写入
    /// `api_key`（恒空串，正常新路径不存 plaintext）、`auth_mode`、`secret_ref`，
    /// 一次 COMMIT 完成。调用方负责在此之前完成 SecretStore.set + read-back verify，
    /// 并在 DB 失败时对新建 secret 做 best-effort 清理。禁止再拆成
    /// `create()` + `set_secret_ref()` 两次提交（防止 `auth_mode=bearer, api_key='',
    /// secret_ref=NULL` 的部分状态）。
    pub fn create_with_secret_ref(
        &self,
        display_name: &str,
        adapter_kind: &AdapterKind,
        base_url: &str,
        api_key: &str,
        model: &str,
        thinking_mode: &ThinkingMode,
        auth_mode: &str,
        secret_ref: Option<&str>,
    ) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO ai_provider_profiles
             (display_name, adapter_kind, base_url, api_key, model, thinking_mode, auth_mode, secret_ref)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                display_name.trim(),
                adapter_kind.as_str(),
                base_url.trim(),
                api_key.trim(),
                model.trim(),
                thinking_mode.as_str(),
                auth_mode,
                secret_ref
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// 编辑 Connection。能力相关字段变化 → compatibility → untested / capabilities → {} /
    /// last_tested_at → NULL（§21）；只改 display_name 不清除兼容结果。
    /// DEV-0062R §18 Disable Guard：Active Primary / Explicit Control 禁止直接停用；
    /// 停用任何连接都不得清空最后一个 enabled。
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: i64,
        display_name: &str,
        adapter_kind: &AdapterKind,
        base_url: &str,
        api_key: &str,
        model: &str,
        thinking_mode: &ThinkingMode,
        auth_mode: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let old = self
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if old.enabled && !enabled {
            if self.active_primary_id() == Some(id) {
                return Err("该连接是当前主要 AI，请先切换主要 AI 后再停用。".to_string());
            }
            if self.active_control_id() == Some(id) {
                return Err(
                    "该连接是当前动作理解 AI，请先改为跟随主要 AI或切换 Control 后再停用。"
                        .to_string(),
                );
            }
            let remaining_enabled: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM ai_provider_profiles WHERE enabled = 1 AND id != ?1",
                    params![id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if remaining_enabled == 0 {
                return Err("至少保留一个可用的 AI 连接。".to_string());
            }
        }
        let capability_changed = old.adapter_kind != adapter_kind.as_str()
            || old.base_url.trim() != base_url.trim()
            || old.api_key.trim() != api_key.trim()
            || old.model.trim() != model.trim()
            || old.thinking_mode != thinking_mode.as_str()
            || old.auth_mode != auth_mode;
        self.conn
            .execute(
                "UPDATE ai_provider_profiles
                 SET display_name=?2, adapter_kind=?3, base_url=?4, api_key=?5, model=?6,
                     thinking_mode=?7, enabled=?8,
                     compatibility_status=CASE WHEN ?9 THEN 'untested' ELSE compatibility_status END,
                     capabilities_json=CASE WHEN ?9 THEN '{}' ELSE capabilities_json END,
                     last_tested_at=CASE WHEN ?9 THEN NULL ELSE last_tested_at END,
                     last_test_message=CASE WHEN ?9 THEN '' ELSE last_test_message END,
                     auth_mode=?10,
                     updated_at=datetime('now')
                 WHERE id=?1",
                params![
                    id,
                    display_name.trim(),
                    adapter_kind.as_str(),
                    base_url.trim(),
                    api_key.trim(),
                    model.trim(),
                    thinking_mode.as_str(),
                    enabled as i64,
                    capability_changed as i64,
                    auth_mode
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// POST-M7 hotfix P0-3：编辑 DB-atomic——**单语句 UPDATE** 一次性原子写入 editable
    /// 字段 + `auth_mode` + `api_key` + `secret_ref` + 兼容重置（compatibility_changed
    /// 由调用方在锁外内存比较得出，见 `create_ai_provider_profile`/
    /// `update_ai_provider_profile`）。一次 COMMIT 完成，禁止再拆成
    /// `update()` + `set_secret_ref()` 两次提交（防止「新 auth_mode + 旧 secret_ref」、
    /// 「配置已改但 secret_ref 缺失」等部分状态）。SecretStore I/O 由调用方在锁外完成。
    #[allow(clippy::too_many_arguments)]
    pub fn update_with_secret_ref(
        &self,
        id: i64,
        display_name: &str,
        adapter_kind: &AdapterKind,
        base_url: &str,
        api_key: &str,
        model: &str,
        thinking_mode: &ThinkingMode,
        auth_mode: &str,
        enabled: bool,
        secret_ref: Option<&str>,
        capability_changed: bool,
    ) -> Result<(), String> {
        let old = self
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if old.enabled && !enabled {
            if self.active_primary_id() == Some(id) {
                return Err("该连接是当前主要 AI，请先切换主要 AI 后再停用。".to_string());
            }
            if self.active_control_id() == Some(id) {
                return Err(
                    "该连接是当前动作理解 AI，请先改为跟随主要 AI或切换 Control 后再停用。"
                        .to_string(),
                );
            }
            let remaining_enabled: i64 = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM ai_provider_profiles WHERE enabled = 1 AND id != ?1",
                    params![id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if remaining_enabled == 0 {
                return Err("至少保留一个可用的 AI 连接。".to_string());
            }
        }
        self.conn
            .execute(
                "UPDATE ai_provider_profiles
                 SET display_name=?2, adapter_kind=?3, base_url=?4, api_key=?5, model=?6,
                     thinking_mode=?7, enabled=?8,
                     compatibility_status=CASE WHEN ?9 THEN 'untested' ELSE compatibility_status END,
                     capabilities_json=CASE WHEN ?9 THEN '{}' ELSE capabilities_json END,
                     last_tested_at=CASE WHEN ?9 THEN NULL ELSE last_tested_at END,
                     last_test_message=CASE WHEN ?9 THEN '' ELSE last_test_message END,
                     auth_mode=?10, secret_ref=?11,
                     updated_at=datetime('now')
                 WHERE id=?1",
                params![
                    id,
                    display_name.trim(),
                    adapter_kind.as_str(),
                    base_url.trim(),
                    api_key.trim(),
                    model.trim(),
                    thinking_mode.as_str(),
                    enabled as i64,
                    capability_changed as i64,
                    auth_mode,
                    secret_ref
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM ai_provider_profiles WHERE id=?1", params![id])?;
        Ok(())
    }

    /// POST-M7 §S3：设置/清除凭据引用（正常新路径永不再写 api_key 列）。
    pub fn set_secret_ref(&self, id: i64, secret_ref: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE ai_provider_profiles SET secret_ref=?2, updated_at=datetime('now') WHERE id=?1",
            params![id, secret_ref],
        )?;
        Ok(())
    }

    /// 保存 Compatibility Probe 结果（§19/§20：不写 API Key；message 已人话化）。
    pub fn save_probe_result(
        &self,
        id: i64,
        caps: &AiCapabilities,
        status: &str,
        message: &str,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE ai_provider_profiles
             SET capabilities_json=?2, compatibility_status=?3, last_test_message=?4,
                 last_tested_at=datetime('now'), updated_at=datetime('now')
             WHERE id=?1",
            params![
                id,
                serde_json::to_string(caps).unwrap_or_else(|_| "{}".into()),
                status,
                message.chars().take(300).collect::<String>()
            ],
        )?;
        Ok(())
    }

    // ---- active 引用（settings KV；§12/§22） ----

    fn read_active_id(&self, key: &str) -> Option<i64> {
        self.conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
    }

    fn write_active_id(&self, key: &str, value: Option<i64>) -> rusqlite::Result<()> {
        match value {
            Some(id) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, id.to_string()],
            )?,
            None => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, '')
                 ON CONFLICT(key) DO UPDATE SET value = ''",
                params![key],
            )?,
        };
        Ok(())
    }

    pub fn active_primary_id(&self) -> Option<i64> {
        self.read_active_id(KEY_ACTIVE_PRIMARY)
    }

    /// 显式 Control id；空 = Follow Primary（§12）。
    pub fn active_control_id(&self) -> Option<i64> {
        self.read_active_id(KEY_ACTIVE_CONTROL)
    }

    /// §22 Primary：incompatible 禁止；untested 仅允许「本来就是 active 的迁移 Connection」
    /// 维持（requested id == current active primary），切到**新的** untested 必须先 Probe
    /// （DEV-0062R §3.13/§17.1：实现修正为注释声明的真实语义）。
    pub fn set_active_primary(&self, id: i64) -> Result<(), String> {
        let p = self
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if !p.enabled {
            return Err("该 AI 连接已被停用，不能设为主要 AI。".to_string());
        }
        if p.compatibility_status == "incompatible" {
            return Err(
                "该 AI 连接未通过 Higher 兼容检测（不兼容），不能设为主要 AI。".to_string(),
            );
        }
        if p.compatibility_status == "untested" && self.active_primary_id() != Some(id) {
            return Err(
                "该 AI 连接尚未检测 Higher 兼容性。请先在连接上运行「检测 Higher 兼容性」。"
                    .to_string(),
            );
        }
        self.write_active_id(KEY_ACTIVE_PRIMARY, Some(id))
            .map_err(|e| e.to_string())
    }

    /// §22 Control：显式 Control 必须 control-compatible（basic_chat + structured_json +
    /// temperature_zero 全 true）且 enabled；None = Follow Primary。
    pub fn set_active_control(&self, id: Option<i64>) -> Result<(), String> {
        match id {
            None => self
                .write_active_id(KEY_ACTIVE_CONTROL, None)
                .map_err(|e| e.to_string()),
            Some(cid) => {
                let p = self
                    .get(cid)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
                if !p.enabled {
                    return Err("该 AI 连接已被停用，不能设为动作理解 AI。".to_string());
                }
                if !p.capabilities.control_compatible() {
                    return Err(
                        "该 AI 连接未满足动作理解要求（基础对话 / 结构化 JSON / 温度 0），不能设为动作理解 AI。"
                            .to_string(),
                    );
                }
                self.write_active_id(KEY_ACTIVE_CONTROL, Some(cid))
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// DEV-0062R §17 Primary / Control 原子切换：先校验两者，再单事务写入；
    /// 任一步失败 → Primary / Control 原值均不变（禁止 partial state）。
    pub fn set_active_profiles_atomic(
        &self,
        primary_id: i64,
        control_id: Option<i64>,
    ) -> Result<(), String> {
        // Validate Primary（§17.1：含 untested 同 id 保持豁免）
        let p = self
            .get(primary_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if !p.enabled {
            return Err("该 AI 连接已被停用，不能设为主要 AI。".to_string());
        }
        if p.compatibility_status == "incompatible" {
            return Err(
                "该 AI 连接未通过 Higher 兼容检测（不兼容），不能设为主要 AI。".to_string(),
            );
        }
        if p.compatibility_status == "untested" && self.active_primary_id() != Some(primary_id) {
            return Err(
                "该 AI 连接尚未检测 Higher 兼容性。请先在连接上运行「检测 Higher 兼容性」。"
                    .to_string(),
            );
        }
        // Validate Control（§17.2）
        if let Some(cid) = control_id {
            let c = self
                .get(cid)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
            if !c.enabled {
                return Err("该 AI 连接已被停用，不能设为动作理解 AI。".to_string());
            }
            if !c.capabilities.control_compatible() {
                return Err(
                    "该 AI 连接未满足动作理解要求（基础对话 / 结构化 JSON / 温度 0），不能设为动作理解 AI。"
                        .to_string(),
                );
            }
        }
        // 单事务写入（BEGIN IMMEDIATE … COMMIT；失败 ROLLBACK）
        self.conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let write = (|| -> rusqlite::Result<()> {
            self.write_active_id(KEY_ACTIVE_PRIMARY, Some(primary_id))?;
            self.write_active_id(KEY_ACTIVE_CONTROL, control_id)?;
            Ok(())
        })();
        match write {
            Ok(()) => {
                self.conn
                    .execute_batch("COMMIT")
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e.to_string())
            }
        }
    }

    /// §38 删除守卫：active Primary / 显式 Control 不能直接删除；
    /// 禁止删除最后一个可用 Connection 后留下幽灵 active id。
    pub fn delete_guarded(&self, id: i64) -> Result<(), String> {
        if self.active_primary_id() == Some(id) {
            return Err("该连接是当前主要 AI，请先切换主要 AI 后再删除。".to_string());
        }
        if self.active_control_id() == Some(id) {
            return Err("该连接是当前动作理解 AI，请先改回「跟随主要 AI」后再删除。".to_string());
        }
        let total_enabled: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM ai_provider_profiles WHERE enabled = 1 AND id != ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let target = self
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "该 AI 连接不存在。".to_string())?;
        if target.enabled && total_enabled == 0 {
            return Err("至少保留一个可用的 AI 连接。".to_string());
        }
        self.delete(id).map_err(|e| e.to_string())
    }
}
