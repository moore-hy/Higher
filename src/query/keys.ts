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

  /**
   * HIGHER COGNITIVE CORE V1.2 §20：Today Coach 单一后端视图。
   *
   * 与 `learningState` / `nextAction` 是**同一份学习真相的不同投影**：
   * 任何可能改变 LearningState / LearningMoments / Memory reviews / 已完成会话的动作，
   * 都必须同时失效 `learningState` + `nextAction` + `review` + `companion` + `cognitiveToday`
   * （闭环失效规则，§20）。
   *
   * 纯 Companion 交互**不得**反向失效本 key（Companion 不拥有学习真相）。
   */
  cognitiveToday: {
    /** 该档案的全部 Today 视图（前缀失效）。 */
    scope: (profileId: number) => ["cognitiveToday", profileId] as const,
    /** 具体时长的视图（时长变了 → 只重取这一档）。 */
    view: (profileId: number, availableMinutes: number | null) =>
      ["cognitiveToday", profileId, availableMinutes] as const,
  },

  /**
   * HIGHER COGNITIVE CORE V1.2 §25：Memory 页单一后端视图。
   *
   * 与 `cognitiveToday` 消费**同一份**记忆真相（同一张 `memory_units` /
   * `memory_reviews`）。因此凡是会改变记忆排程的动作（完成一次复习 =
   * 新增 LearningMoment 并推进排程），都必须同时失效两者——
   * 否则 Memory 页会停在旧的到期队列上。
   */
  cognitiveMemory: {
    /** 该档案的全部 Memory 视图（前缀失效）。 */
    scope: (profileId: number) => ["cognitiveMemory", profileId] as const,
    /** 具体 limit 的视图。 */
    view: (profileId: number, limit: number) =>
      ["cognitiveMemory", profileId, limit] as const,
  },

  /**
   * HIGHER COGNITIVE CORE V1.2 §26：Progress 页四轴视图。
   *
   * 与 `cognitiveToday` / `cognitiveMemory` 同源（学习时刻 + 记忆排程 + 完成会话）。
   * 任何会改变这三者的动作都必须同时失效本 key，否则四轴会停在旧数字上。
   */
  cognitiveProgress: {
    /**
     * 该档案的四轴视图。
     *
     * 本视图只有「档案」一个维度（不像 Today 还有时长档），因此
     * **同一个 key 既用于查询也用于前缀失效**，不额外造一个 `view`。
     */
    scope: (profileId: number) => ["cognitiveProgress", profileId] as const,
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

  /**
   * REAL LEARNING ENGINE V1 · W4 —— TrainingExperience。
   *
   * 一次训练的全部状态由**一个** key 承载（run + 块 + 交互），
   * 与后端 `get_training_session` 的「一次 IPC 返回整页」契约一致。
   */
  training: {
    session: (profileId: number, trainingRunId: number) =>
      ["training", "session", profileId, trainingRunId] as const,
    /**
     * GROUNDED LEARNING BRIDGE V1 · W5 §10 —— 某个训练块的接地材料快照。
     *
     * 按块 id 建键：切块时各自独立缓存，不会把上一段的材料带进下一段。
     * 快照在落库后是不可变的，因此这个键天然稳定（无需失效策略）。
     */
    blockMaterial: (profileId: number, blockRunId: number) =>
      ["training", "block-material", profileId, blockRunId] as const,
  },
} as const;

export type QueryKeys = typeof queryKeys;
