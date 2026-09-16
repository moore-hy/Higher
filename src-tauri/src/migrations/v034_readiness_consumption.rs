//! v034 · P0-01 就绪度消费水位线（Readiness Consumption Watermark）。
//!
//! ## 为什么需要这条迁移
//!
//! M5 的远征就绪度由 M3 `today_total` 派生（§M5-C）。但此前「开始远征」**没有真正消费**
//! 这次就绪机会：同一份学习证据在「出发 → 收取 → 再出发」的循环里会被反复兑现，
//! 等价于一个可被无限刷新的能量钱包（§M4-C / §M5-C 明确禁止）。
//!
//! 修复是在 `companion_world_state` 上加两个**有界、与学习日绑定**的水位线列，
//! 仅用于表达「哪些已有贡献已被当前远征机会兑现」，绝不建模余额/币/燃料：
//!
//! ```text
//! consumed_local_date         -> 上一次出发所消耗的学习日（本地日期 yyyy-mm-dd）
//! consumed_contribution_total -> 该次出发时 today_total 的快照值
//! ```
//!
//! 语义（详见 `companion::service::build_companion_state_at`）：
//!
//! ```text
//! if current_local_date != consumed_local_date:
//!     # 新学习日：当日贡献重新起算，绝不拿今天和昨天的旧水位线相减
//!     unconsumed = today_total
//! else:
//!     unconsumed = max(0, today_total - consumed_contribution_total)
//! derived = readiness_from_contribution(unconsumed)
//! ```
//!
//! 这是「最小前向安全」机制：ALTER 两张列、默认值 NULL / 0，不触碰 v033，
//! 不引入任何钱包语义。已存在（v033）的 companion 数据在新列上得到确定性默认值，
//! 不会被误锁（consumed_local_date 为 NULL → 视为无消费 → 全额派生）。

use rusqlite::Connection;

pub fn up(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        // 只增列，不重建表：v033 的全部行（profile / expedition / memory）原样保留。
        "ALTER TABLE companion_world_state
            ADD COLUMN consumed_local_date TEXT;

         ALTER TABLE companion_world_state
            ADD COLUMN consumed_contribution_total INTEGER NOT NULL DEFAULT 0;

         PRAGMA foreign_key_check;",
    )
}
