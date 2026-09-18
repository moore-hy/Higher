//! v043 · Grounded Training Material Snapshot（GROUNDED LEARNING BRIDGE V1 · W3）。
//!
//! # 这张迁移做什么
//!
//! 只给 §10 的 `training_block_runs` 增加**一列**：
//!
//! ```text
//! material_snapshot_json  TEXT NULL
//! ```
//!
//! 落地「一次训练块真正用到的接地材料快照」（§8）。训练历史必须可复现：
//! 用户事后重新导入 PDF 不得悄悄改变一个已创建训练块的语义。
//!
//! # 为什么只是加一列，不是新建表
//!
//! 快照是「内容」不是「学习真相」。它钉在某一个已存在的训练块行上，
//! 不引入第二份训练状态真相源（与 `training` 模块「不建第二真相源」纪律一致，§8.3）。
//!
//! # 既有行自然回填 NULL
//!
//! 新增列 **无 NOT NULL 约束**，因此所有已经存在的 `training_block_runs` 行在迁移后
//! 该列自动为 NULL —— 没有任何历史行被改写成假材料（满足 §8「不得伪造历史行」）。
//!
//! # 不在本迁移里做的事
//!
//! - 不做任何整数 ID → 字符串的「美化重命名」（§8 明确禁止）。
//! - 不新建 v044+（本任务是授权新增 v043 的唯一机会）。
//! - 不触碰文档表、不触碰 `learning_moments` / `evidence` / `memory_reviews`。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    // 既有行因列无 NOT NULL 约束而自然回填 NULL；不写入任何假材料。
    conn.execute_batch(
        "ALTER TABLE training_block_runs ADD COLUMN material_snapshot_json TEXT NULL;",
    )?;
    Ok(())
}
