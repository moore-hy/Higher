// DEV-0050 · PHASE A 复现专用浏览器 mock（仅 vite dev server 加载，不进 Tauri 构建）
// 用法：vite.config.ts 中仅当 !process.env.TAURI_ENV_PLATFORM 时注入 <script src="/mock/inject.js">
// 作用：在浏览器里模拟 @tauri-apps/api invoke，让 LearningWorkspace 全链可操作复现两个 P0。
(function () {
  if (window.__TAURI_INTERNALS__) return; // 真实 Tauri 环境不注入

  // ---- 内存数据库（最小闭环子集） ----
  let profileId = 1;
  let sessionSeq = 1;
  let session = null; // active session
  const sessions = [];
  const notes = new Map(); // id -> {note, doc}
  const tasks = [];
  const goals = [{ id: 1, profile_id: 1, name: "G", goal_level: "legacy", status: "active" }];

  const ok = (v) => ({ status: 0, content: [{ type: "text", text: JSON.stringify(v ?? null) }] });

  const handlers = {
    // Profile gate
    list_study_profiles: () => [{ id: profileId, name: "复现档案", profile_type: null, status: "active" }],
    get_active_study_profile: () => ({ id: profileId, name: "复现档案" }),
    has_active_session: () => !!session,
    // Learning loop
    start_quick_session: () => {
      session = {
        id: sessionSeq, profile_id: profileId, goal_id: null, task_id: null, learning_item_id: null,
        title: "快速学习", started_at: new Date().toISOString().slice(0, 19).replace("T", " "),
        ended_at: null, duration_seconds: null, status: "active", note: null, note_document_json: null,
        created_at: "", updated_at: "", time_corrected: 0,
      };
      sessions.push(session);
      return session;
    },
    get_session: ({ id }) => {
      const s = sessions.find((x) => x.id === id);
      if (!s) return null;
      const n = notes.get(id);
      return { ...s, note: n ? n.note : s.note, note_document_json: n ? n.doc : s.note_document_json };
    },
    update_session_document: ({ sessionId, note, noteDocumentJson }) => {
      notes.set(sessionId, { note, doc: noteDocumentJson });
      return null;
    },
    end_session: ({ id }) => {
      const s = sessions.find((x) => x.id === id);
      s.status = "completed";
      s.ended_at = new Date().toISOString().slice(0, 19).replace("T", " ");
      s.duration_seconds = 268;
      const n = notes.get(id);
      if (n) { s.note = n.note; s.note_document_json = n.doc; }
      session = null;
      sessionSeq++;
      return s;
    },
    // 附属数据
    list_learning_items_by_profile: () => [],
    list_goals_by_profile: () => goals,
    list_all_tasks_by_profile: () => tasks,
    list_today_tasks_by_profile: () => tasks,
    list_attachments_by_session: () => [],
    get_learning_item_path: () => "知识",
    update_session_title: () => null,
    materialize_recurring_tasks: () => 0,
    sync_notifications: () => null,
    get_notification_enabled: () => true,
    // AI（避免点击 AI 按钮时网络报错刷屏）
    get_ai_settings: () => ({ provider: "deepseek", base_url: "https://api.deepseek.com", api_key: "", model: "deepseek-v4-flash", thinking_enabled: false }),
    get_ui_setting: () => null,
    get_active_session: () => null,
    get_profile_day_sessions: () => [],
    list_recurring_rules_by_profile: () => [],
    get_profile_day_evaluations: () => [],
    // DEV-0050 默认（数组类命令缺省返回 []，避免组件 null 崩）
    list_recent_sessions_by_profile: () => [],
    list_tasks_by_range_by_profile: () => [],
    get_profile_range_sessions: () => [],
    get_goal_tree: () => ({
      final_goal: {
        id: 1, name: "未设置最终目标", description: null, status: "active", profile_id: 1,
        parent_goal_id: null, goal_level: "final", period_start: null, period_end: null, sort_order: 0,
        created_at: "", updated_at: "",
        children: [],
      },
      legacy_goals: [],
    }),
    get_learning_trend_v2: () => [],
    get_latest_mastery: () => ({ assessment: null, stale: false }),
    get_legacy_planning_counts: () => [0, 0],
    list_plans: () => [],
    list_study_stages: () => [],
    ai_analyze: () => { throw new Error("mock: AI 未配置（复现环境）"); },
  };

  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      const h = handlers[cmd];
      if (h) return h(args ?? {});
      console.warn("[mock] unhandled invoke:", cmd);
      return null;
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    plugins: {},
  };
  console.log("[mock] Tauri browser shim injected (DEV-0050 PHASE-A 复现)");
})();
