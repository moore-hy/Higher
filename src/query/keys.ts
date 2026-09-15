/**
 * Foundation 2.0 §7.3：Query Key 单一事实源。
 *
 * 规则：
 * - 同一实体禁止在不同页面随意制造不同 key；
 * - 所有 profile-scoped key 必须包含 profileId（避免跨 Profile 缓存串号）；
 * - key 结构保持 `[domain, kind, ...scope]`，便于按前缀精准 invalidate。
 */
export const queryKeys = {
  profiles: {
    all: ["profiles", "all"] as const,
    active: ["profiles", "active"] as const,
  },

  goals: {
    tree: (profileId: number) => ["goals", "tree", profileId] as const,
    list: (profileId: number) => ["goals", "list", profileId] as const,
  },

  tasks: {
    today: (profileId: number, date: string) => ["tasks", "today", profileId, date] as const,
    range: (profileId: number, from: string, to: string) =>
      ["tasks", "range", profileId, from, to] as const,
    archived: (profileId: number) => ["tasks", "archived", profileId] as const,
  },

  sessions: {
    active: (profileId: number) => ["sessions", "active", profileId] as const,
    recent: (profileId: number) => ["sessions", "recent", profileId] as const,
    byDate: (profileId: number, date: string) => ["sessions", "byDate", profileId, date] as const,
  },

  /** §7.5 sync://completed 需按 payload 精准 invalidate learningItems。 */
  learningItems: {
    byProfile: (profileId: number) => ["learningItems", "byProfile", profileId] as const,
    light: (profileId: number) => ["learningItems", "light", profileId] as const,
  },

  knowledge: {
    workspace: (profileId: number) => ["knowledge", "workspace", profileId] as const,
    documents: (profileId: number) => ["knowledge", "documents", profileId] as const,
    item: (profileId: number, itemId: number) =>
      ["knowledge", "item", profileId, itemId] as const,
  },

  report: {
    daily: (profileId: number, date: string) => ["report", "daily", profileId, date] as const,
  },

  /**
   * HIGHER CLOSED LOOP V1（PHASE 1 / 2 / 8）：
   * Today / Review 的闭环数据只走这两个 key —— 禁止再用 refreshKey 驱动刷新。
   *
   * 失效策略（精准 invalidate，不整库刷）：
   * - Session End / Task Complete / Review Apply → `learningState.all(profileId)`
   *   （快照里同时含 today_tasks / active_session / recent_sessions / evidence）
   * - 时间档变化 → 只重取 `nextAction`（预算变了，快照没变）
   */
  learningState: {
    all: (profileId: number) => ["learningState", profileId] as const,
  },

  nextAction: {
    /** budget = null 表示「未选择时间档」。 */
    for: (profileId: number, budget: string | null) =>
      ["nextAction", profileId, budget] as const,
    /** 该档案的全部时间档（结束/完成任务后一次性作废）。 */
    scope: (profileId: number) => ["nextAction", profileId] as const,
  },

  /**
   * M1-A：有限 Learning Pack（1..=3 条 canonical 候选的截断视图）。
   *
   * 与 `nextAction` 使用同一份快照与同一套排序 —— 二者必须一起失效，
   * 否则会出现「Primary 已更新、Pack 还是旧的」这种自相矛盾状态。
   * 因此失效时同时 invalidate `learningPack` + `nextAction`（见 Today 页）。
   */
  learningPack: {
    for: (profileId: number, budget: string | null) =>
      ["learningPack", profileId, budget] as const,
    scope: (profileId: number) => ["learningPack", profileId] as const,
  },

  /**
   * PHASE 8：Review 的闭环数据（观察窗口内的 Task / Session / Evaluation /
   * Feedback / Adjustment）。profile-scoped + 窗口 scoped。
   *
   * Session End / Task Complete / Review Apply → invalidate `review.scope(profileId)`
   * （前缀匹配该档案的所有窗口查询），Review 页面不得再依赖 refreshKey。
   */
  review: {
    range: (profileId: number, start: string | null, end: string | null) =>
      ["review", profileId, "range", start, end] as const,
    scope: (profileId: number) => ["review", profileId] as const,
  },

  /**
   * M4 / M5 / M6：Companion / World 只读投影（独立子系统，独立 lifecycle）。
   *
   * 与 `learningState` **分域**：Companion 有自己的一套状态（身份 / 世界 / 远征 /
   * 记忆 / 对白），只有在「远征开始 / 结算 / 收取」时才变化，因此不复用学习状态的 key。
   *
   * 但两者的失效是**单向联动**的：
   * - 学习闭环事件（Session End / Task Complete / Micro 写入）会改变
   *   Meaningful Contribution → 进而改变远征就绪度，因此学习侧 invalidate 时
   *   必须同时失效 `companion.scope(profileId)`；
   * - Companion 侧的变化**绝不**反向失效学习状态（§M4-A：Companion 不拥有学习真相，
   *   点宠物 / 打招呼 / 远征不会改变任何学习数据）。
   */
  companion: {
    /** §M4-D `get_companion_state`（含世界状态 / 行为 / 就绪度 / 对白）。 */
    state: (profileId: number) => ["companion", "state", profileId] as const,
    /** §M4-D `get_companion_memories`（收藏 / 场景记忆，倒序）。 */
    memories: (profileId: number) => ["companion", "memories", profileId] as const,
    /** 该档案的全部 companion 查询（前缀失效）。 */
    scope: (profileId: number) => ["companion", profileId] as const,
  },

  data: {
    totals: (profileId: number) => ["data", "totals", profileId] as const,
    trends: (profileId: number, range: string) => ["data", "trends", profileId, range] as const,
  },

  settings: {
    ai: ["settings", "ai"] as const,
    notification: ["settings", "notification"] as const,
    webSearch: ["settings", "webSearch"] as const,
    semantic: ["settings", "semantic"] as const,
  },

  sync: {
    workspace: ["sync", "workspace"] as const,
    client: ["sync", "client"] as const,
    server: ["sync", "server"] as const,
  },
} as const;

export type QueryKeys = typeof queryKeys;
