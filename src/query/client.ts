import { QueryClient } from "@tanstack/react-query";

/**
 * Foundation 2.0 §7.2：Tauri IPC server-state 的唯一 QueryClient 单例。
 *
 * 约定：
 * - query 默认 retry = 1（本地 SQLite IPC，瞬时失败重试一次足够）；
 * - mutation 默认 retry = 0（写操作绝不自动重放）；
 * - 本地 IPC query 使用合理 staleTime（本地真值源，无需高频重取）；
 * - 不允许无限自动轮询：refetchInterval 默认关闭，由页面显式、有限使用；
 * - 所有 profile-scoped key 必须包含 profileId（见 keys.ts）。
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 30_000,
      gcTime: 5 * 60_000,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
      refetchInterval: false,
    },
    mutations: {
      retry: 0,
    },
  },
});
