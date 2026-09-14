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
