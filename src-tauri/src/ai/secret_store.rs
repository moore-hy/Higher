//! OS SecretStore 抽象（POST-M7 AI FOUNDATION §S3-A/§S3-B / FINAL IMPLEMENTATION
//! SAFETY PATCH §2）。
//!
//! - 唯一抽象：[`SecretStore`] trait（get / set / delete）。禁止第二套
//!   encrypted_vault_v2 / custom_crypto_store / password_file；
//! - 生产实现：[`OsSecretStore`] —— 复用既有 `keyring` 依赖（Windows 凭据管理器 /
//!   Android Keystore），`service = "Higher"`，`account = "ai-provider:<secret_ref>"`；
//! - 测试实现：[`MemorySecretStore`]（不依赖真实 Windows Credential Manager）；
//! - `ai/vault.rs` 是变更审计日志（Vault），**不是** OS Secure SecretStore；
//! - 永久不变量（S3-D1）：OS 凭据 I/O 永远不得在持有 SQLite 事务 / 应用 DB 锁时执行。
//!   调用方模式：短 DB 读 → 释放锁 → SecretStore I/O → 重新拿锁 → 短事务提交。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// keyring service 名（稳定标识，不改名）。
pub const KEYRING_SERVICE: &str = "Higher";

/// 由 secret_ref 构造 keyring account（稳定方案：`ai-provider:<secret_ref>`）。
pub fn keyring_account(secret_ref: &str) -> String {
    format!("ai-provider:{secret_ref}")
}

/// 唯一 SecretStore 抽象（§S3-B：不建通用密码管理器）。
pub trait SecretStore: Send + Sync {
    /// 读取 secret；不存在 → Ok(None)。失败 → Err（不伪造内容）。
    fn get(&self, secret_ref: &str) -> Result<Option<String>, String>;
    /// 写入 secret（幂等覆盖同 ref）。失败 → Err，调用方**不得**清空旧凭据。
    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), String>;
    /// 删除 secret（best-effort 语义由调用方决定）。不存在时也应 Ok。
    fn delete(&self, secret_ref: &str) -> Result<(), String>;
}

/// 生产实现：OS 原生 Keyring。
pub struct OsSecretStore;

impl SecretStore for OsSecretStore {
    fn get(&self, secret_ref: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(secret_ref))
            .map_err(|e| format!("SecretStore 不可用：{e}"))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_)) => {
                Err(format!("SecretStore 读取失败：{}", platform_message()))
            }
            Err(e) => Err(format!("SecretStore 读取失败：{e}")),
        }
    }

    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(secret_ref))
            .map_err(|e| format!("SecretStore 不可用：{e}"))?;
        entry
            .set_password(secret)
            .map_err(|_| format!("SecretStore 写入失败：{}", platform_message()))
    }

    fn delete(&self, secret_ref: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &keyring_account(secret_ref))
            .map_err(|e| format!("SecretStore 不可用：{e}"))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("SecretStore 删除失败：{e}")),
        }
    }
}

fn platform_message() -> String {
    "操作系统凭据管理器暂不可用，请稍后重试或重新填写 API Key。".to_string()
}

/// 测试实现：内存 Map（不依赖真实 Windows Credential Manager）。
pub struct MemorySecretStore {
    inner: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    pub fn contains(&self, secret_ref: &str) -> bool {
        self.inner.lock().unwrap().contains_key(secret_ref)
    }
}

impl Default for MemorySecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, secret_ref: &str) -> Result<Option<String>, String> {
        Ok(self.inner.lock().unwrap().get(secret_ref).cloned())
    }

    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), String> {
        self.inner
            .lock()
            .unwrap()
            .insert(secret_ref.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, secret_ref: &str) -> Result<(), String> {
        self.inner.lock().unwrap().remove(secret_ref);
        Ok(())
    }
}

/// 进程级唯一生产 store。
static PRODUCTION_STORE: OnceLock<Arc<dyn SecretStore>> = OnceLock::new();

pub fn production_secret_store() -> Arc<dyn SecretStore> {
    PRODUCTION_STORE
        .get_or_init(|| Arc::new(OsSecretStore) as Arc<dyn SecretStore>)
        .clone()
}

/// 生成稳定 secret reference（§S3-C：UUID；display_name 可改，不作 key）。
pub fn generate_secret_ref() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_roundtrip_and_delete() {
        let store = MemorySecretStore::new();
        assert_eq!(store.get("r1").unwrap(), None);
        store.set("r1", "sk-test").unwrap();
        assert_eq!(store.get("r1").unwrap(), Some("sk-test".to_string()));
        store.delete("r1").unwrap();
        assert_eq!(store.get("r1").unwrap(), None);
        // delete 不存在的 ref 也应 Ok
        store.delete("never-existed").unwrap();
    }

    #[test]
    fn secret_ref_is_stable_uuid_shape() {
        let a = generate_secret_ref();
        let b = generate_secret_ref();
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
    }
}
