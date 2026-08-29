//! DEV-SYNC-001 · Local-First LAN Sync MVP。
//!
//! Sync 是旁路基础设施：Business → SQLite → Trigger → Sync，
//! 不侵入 Repository / UI / AI Runtime；Sync 完全关闭时原功能不受影响。
//!
//! - types      wire model（SyncPacket / SyncChange / SyncEntityPayload / WireMessage）
//! - identity   local id ↔ sync_id 映射 + peer / outbox / 冲突查询
//! - export     outbox 增量导出 + Bootstrap 快照（FK → sync_id）
//! - apply      Remote Apply（事务 + guard 防回声 + 冲突守卫）
//! - transport  TCP length-prefixed JSON（4B 长度 + UTF-8 JSON，≤ 10 MB）
//! - server     Windows 同步服务器（配对码 / token / 增量交换）
//! - client     Android 客户端（配对 / Bootstrap 导入 / 立即同步）

pub mod apply;
pub mod client;
pub mod export;
pub mod identity;
pub mod qr;
pub mod server;
pub mod transport;
pub mod types;
