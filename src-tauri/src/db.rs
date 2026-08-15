use rusqlite::Connection;
use std::sync::Mutex;

/// 全局数据库状态，由 Tauri 管理。
///
/// 打开数据库后执行 Migration（由 `migrations` 模块统一管理），
/// 自动判断 schema 版本并按顺序执行尚未执行的 Migration。
/// 业务表（subjects / chapters / ... / plans）由后续 Migration 逐步引入。
pub struct DbState(pub Mutex<Connection>);

impl DbState {
    /// 打开（或创建）指定路径的 SQLite 数据库，并执行待处理的 Migration。
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        // 开启外键约束，后续业务表将依赖外键完整性
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        // 执行数据库 Migration（自动判断版本，按顺序执行未执行的）
        // 表结构（含 settings）由 migrations 模块统一管理
        crate::migrations::run_migrations(&conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    /// 当前数据库文件路径（与 lib.rs 的 dev/prod 约定一致；备份用）。
    pub fn database_path() -> std::path::PathBuf {
        if cfg!(debug_assertions) {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".data")
                .join("higher.db")
        } else {
            // prod：AppData\Local\com.higher.desktop\higher.db（与 lib.rs setup 一致）
            if let Some(local) = std::env::var("LOCALAPPDATA").ok() {
                std::path::PathBuf::from(local)
                    .join("com.higher.desktop")
                    .join("higher.db")
            } else {
                std::path::PathBuf::from("higher.db")
            }
        }
    }
}
