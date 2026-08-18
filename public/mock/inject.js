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
  // DEV-0058：AI 会话消息（ai_start_run mock 追加；list_ai_messages 读取）
  window.__higherMockMessages = [
    { id: 2, conversation_id: 1, profile_id: 1, role: "user", content: "我最近都学习了什么？", run_id: null, created_at: "2026-08-19 10:00:30" },
    { id: 3, conversation_id: 1, profile_id: 1, role: "assistant", content: "根据记录，你最近主要学习了极限与连续 [[S1]]。", run_id: "r1", created_at: "2026-08-19 10:01:00" },
  ];

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
        duration_review_state: "normal",
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
      s.duration_review_state = s.duration_seconds > 43200 ? "needs_review" : "normal";
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
    // DEV-0051 Knowledge Workspace V2
    get_knowledge_workspace: () => ({
      item_id: 1, item_name: "进程管理", mastery_status: "learning",
      documents: [
        { id: 1, profile_id: 1, learning_item_id: 1, title: "进程与线程区别", content_text: "进程是资源分配的基本单位\n线程是调度单位", content_document_json: null, created_at: "2026-08-16 08:20:00", updated_at: "2026-08-16 08:20:00" },
        { id: 2, profile_id: 1, learning_item_id: 1, title: "fork / exec 总结", content_text: "fork 复制进程\nexec 替换映像", content_document_json: null, created_at: "2026-08-18 20:15:00", updated_at: "2026-08-18 20:15:00" },
      ],
      sessions: [
        { id: 3, title: "快速学习", started_at: "2026-08-16 09:20:00", duration_seconds: 780, status: "completed", note_plain: "看了进程章节\n记了笔记", image_count: 4, video_count: 1, attachment_count: 5 },
        { id: 4, title: "media-only", started_at: "2026-08-17 10:00:00", duration_seconds: 300, status: "completed", note_plain: "", image_count: 4, video_count: 0, attachment_count: 4 },
      ],
      legacy_attachments: [
        { id: 10, profile_id: 1, learning_item_id: 1, session_id: null, document_id: null, attachment_type: "image", file_name: "old.png", relative_path: "1/item/old.png", mime_type: null, caption: "", created_at: "" },
      ],
      session_count: 2, study_seconds: 1080, last_studied_at: "2026-08-17 10:00:00", evaluation_count: 0,
    }),
    create_knowledge_document: () => ({ id: 99, profile_id: 1, learning_item_id: 1, title: "未命名文档", content_text: "", content_document_json: null, created_at: "2026-08-19 00:00:00", updated_at: "2026-08-19 00:00:00" }),
    get_knowledge_document: () => null, // 让前端走 list
    list_knowledge_documents: () => [],
    update_knowledge_document: (a) => ({ id: a.id, profile_id: 1, learning_item_id: 1, title: a.title, content_text: a.contentText, content_document_json: a.contentDocumentJson, created_at: "", updated_at: "2026-08-19 00:00:10" }),
    rename_knowledge_document: (a) => ({ id: a.id, profile_id: 1, learning_item_id: 1, title: a.title, content_text: "", content_document_json: null, created_at: "", updated_at: "" }),
    delete_knowledge_document: () => null,
    list_attachments_by_document: () => [],
    // Knowledge 页既有
    list_learning_items_by_profile: () => [{ id: 1, profile_id: 1, parent_id: null, name: "进程管理", mastery_status: "learning", content: "", sort_order: 0, created_at: "", updated_at: "", goal_id: 1 }],
    list_learning_items_by_goal: () => [{ id: 1, profile_id: 1, parent_id: null, name: "进程管理", mastery_status: "learning", content: "", sort_order: 0, created_at: "", updated_at: "", goal_id: 1 }],
    get_learning_item_path: () => "111 › 222",
    ai_analyze: () => { throw new Error("mock: AI 未配置（复现环境）"); },
    // DEV-0057 · Reliability / Data Trust / Performance
    list_learning_items_light: () => [
      { id: 1, goal_id: 1, parent_id: null, name: "进程管理", mastery_status: "learning", sort_order: 0, created_at: "", updated_at: "" },
    ],
    get_attachment_asset_path: ({ attachmentId }) => `C:/HigherMock/attachments/${attachmentId ?? 1}.png`,
    read_attachment_image: ({ id }) => ({
      id, file_name: `att-${id}.png`, mime_type: "image/png",
      base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
    }),
    confirm_session_duration: ({ id }) => {
      const s = sessions.find((x) => x.id === id);
      if (s) { s.duration_review_state = "confirmed"; return { ...s }; }
      return { id, profile_id: profileId, goal_id: null, task_id: null, learning_item_id: null, title: "快速学习", started_at: "", ended_at: "", duration_seconds: 0, status: "completed", note: null, note_document_json: null, created_at: "", updated_at: "", time_corrected: 0, activity_kind: "unplanned", duration_review_state: "confirmed" };
    },
    rebuild_search_index: () => 0,
    // DEV-0052 Personal Intelligence（冒烟 mock）
    get_ai_mode: () => "readonly",
    set_ai_mode: () => null,
    create_ai_conversation: () => ({ id: 1, profile_id: 1, title: "新对话", mode: "readonly", created_at: "2026-08-19 10:00:00", updated_at: "2026-08-19 10:00:00", archived_at: null }),
    list_ai_conversations: () => [
      { id: 1, profile_id: 1, title: "极限学习讨论", mode: "readonly", created_at: "2026-08-19 10:00:00", updated_at: "2026-08-19 11:00:00", archived_at: null },
      { id: 2, profile_id: 1, title: "规划咨询", mode: "assistant", created_at: "2026-08-18 09:00:00", updated_at: "2026-08-18 09:30:00", archived_at: null },
    ],
    archive_ai_conversation: () => null,
    set_ai_conversation_mode: () => null,
    search_higher: () => [],
    list_memory_records: () => [],
    dismiss_memory_record: () => null,
    // DEV-0058：changeset mock 定义移至下方（get_change_set_apply_summary 之后）
    import_personalization_files: () => [],
    list_personalization_sources: () => [],
    delete_personalization_source: () => null,
    get_personalization_profile: () => null,
    compile_personalization: () => { throw new Error("mock: 需真实 AI"); },
    confirm_personalization_profile: () => null,
    edit_personalization_profile: () => null,
    get_requirement_template: () => "# Higher AI 需求采集模板（mock）",
    get_web_search_settings: () => [false, false],
    set_web_search_settings: () => null,
    vault_status: () => ({ locked: true, hint: "测试版密码为 root", stats: null }),
    vault_unlock: () => null,
    vault_lock: () => null,
    vault_list_events: () => [],
    vault_list_snapshots: () => [],
    vault_create_snapshot: () => 1,
    vault_export_events: () => "[]",
    // DEV-0058：mock 规划 run（浏览器 smoke：写意图→追加会话消息→返回 run id）
    ai_start_run: async (a) => {
      const msg = String(a?.userMessage ?? "");
      const writeIntent = /安排|排个|排一下|加入\s*higher|计划/i.test(msg);
      const rid = "mock-run-" + Date.now();
      const M = window.__higherMockMessages;
      M.push({ id: M.length + 1, conversation_id: 1, profile_id: 1, role: "user", content: msg, run_id: rid, created_at: new Date().toISOString() });
      const reply = writeIntent
        ? "已经准备好一份可执行计划。\n计划范围：8月18日 → 8月20日\n本次将：\n新增 2 个阶段目标\n新增 3 个知识节点\n安排 3 个学习任务\n包含 1 个休息日\n点击「查看计划」审查后应用；未应用前 Higher 数据不会变化。"
        : "（mock 回复）这条消息在真实环境中会由 AI 回答。";
      M.push({ id: M.length + 1, conversation_id: 1, profile_id: 1, role: "assistant", content: reply, run_id: rid, created_at: new Date().toISOString() });
      return rid;
    },
    ai_cancel_run: () => true,
    ai_active_run_count: () => 0,
    open_external_url: () => null,
    // DEV-0053 Daily & Dual-Tree
    get_daily_learning_report: () => ({
      date: "2026-08-16", planned_minutes: 180, unestimated_task_count: 1, actual_minutes: 110,
      planned_task_actual_minutes: 70, task_total: 3, task_completed: 1,
      task_completion_rate: 33.3, day_goal: "8月16日学习", day_goal_id: 4, day_goal_progress: 40,
      time_execution_rate: 61.1, overall_efficiency: 45.0, learning_status: "计划执行偏低",
      tasks: [
        { id: 11, title: "极限定义", status: "pending", planned_time: "20:00", estimated_minutes: 60, task_kind: "structured", priority: "core", goal_id: 4, learning_item_id: 1, knowledge_name: "高数 / 极限", deep_link: "higher://task/11" },
        { id: 12, title: "二叉树", status: "pending", planned_time: null, estimated_minutes: 45, task_kind: "structured", priority: "normal", goal_id: 4, learning_item_id: 1, knowledge_name: "数据结构", deep_link: "higher://task/12" },
        { id: 13, title: "背20个单词", status: "completed", planned_time: null, estimated_minutes: 20, task_kind: "accumulation", priority: "normal", goal_id: 4, learning_item_id: null, knowledge_name: null, deep_link: "higher://task/13" },
      ],
      needs_review_count: 0,
      activities: [
        { id: 21, title: "极限定义", started_at: "2026-08-16 02:20:00", duration_seconds: 3120, activity_kind: "core", learning_item_id: 1, task_id: 11, deep_link: "higher://session/21", duration_review_state: "normal" },
        { id: 22, title: "快速学习", started_at: "2026-08-16 06:30:00", duration_seconds: 2400, activity_kind: "unplanned", learning_item_id: null, task_id: null, deep_link: "higher://session/22", duration_review_state: "normal" },
      ],
    }),
    list_unassigned_sessions: () => [
      { id: 22, profile_id: 1, goal_id: null, task_id: null, learning_item_id: null, title: "快速学习", started_at: "2026-08-16 06:30:00", ended_at: "2026-08-16 07:10:00", duration_seconds: 2400, status: "completed", note: null, note_document_json: null, created_at: "", updated_at: "", time_corrected: 0, activity_kind: "unplanned" },
    ],
    organize_session_into_knowledge: () => null,
    set_session_activity_kind: () => null,
    set_session_goal: () => null,
    create_followup_task_from_session: () => ({ id: 99, profile_id: 1, goal_id: null, learning_item_id: null, title: "继续：快速学习", planned_date: "2026-08-17", planned_time: null, status: "pending", archived_at: null, plan_id: null, recurring_rule_id: null, created_at: "", updated_at: "", estimated_minutes: 30, task_kind: "structured", priority: "normal" }),
    list_sessions_by_goal: () => [
      { id: 21, profile_id: 1, goal_id: 4, task_id: 11, learning_item_id: 1, title: "极限定义", started_at: "2026-08-16 02:20:00", ended_at: "2026-08-16 03:12:00", duration_seconds: 3120, status: "completed", note: null, note_document_json: null, created_at: "", updated_at: "", time_corrected: 0, activity_kind: "core" },
    ],
    create_task_v2: (a) => ({ id: 100, profile_id: 1, goal_id: a?.goalId ?? null, learning_item_id: a?.learningItemId ?? null, title: a?.title ?? "", planned_date: a?.plannedDate ?? null, planned_time: a?.plannedTime ?? null, status: "pending", archived_at: null, plan_id: null, recurring_rule_id: null, created_at: "", updated_at: "", estimated_minutes: a?.estimatedMinutes ?? null, task_kind: a?.taskKind ?? "structured", priority: a?.priority ?? "normal" }),
    update_task_v2: () => null,
    get_change_set_apply_summary: () => ["✓ 计划已应用", "新增 2 个目标", "新增 3 个知识节点", "新增 12 个任务"],
    // DEV-0058：mock changeset（浏览器 smoke 用；计划形态：阶段+知识+按日任务+休息日）
    get_ai_change_set: (a) => ({ id: a?.changeSetId ?? 1, profile_id: 1, conversation_id: 1, run_id: "mock", title: "学习计划（18 项）", summary: "阶段目标 +2 · 知识节点 +3 · 学习任务 +12 · 休息日 2", status: "pending", created_at: "2026-08-17 10:00:00", applied_at: null, rejected_at: null }),
    list_ai_change_set_operations: (a) => {
      const n = (a?.changeSetId ?? 1) * 100;
      const t = (id, title, date, min, k) => ({ id: n + id, change_set_id: a?.changeSetId ?? 1, entity_type: "task", entity_id: null, action: "create", after_json: { title, date, estimated_minutes: min, task_kind: k, priority: "normal" }, before_json: null, reason: "", operation_ref: null, selected: 1, deep_link: "" });
      const g = (id, lvl, name, period, rest) => ({ id: n + id, change_set_id: a?.changeSetId ?? 1, entity_type: "goal", entity_id: null, action: "create", after_json: { goal_level: lvl, name, period, rest_day: rest ? true : undefined }, before_json: null, reason: "", operation_ref: null, selected: 1, deep_link: "" });
      const k = (id, name) => ({ id: n + id, change_set_id: a?.changeSetId ?? 1, entity_type: "knowledge", entity_id: null, action: "create", after_json: { name }, before_json: null, reason: "", operation_ref: null, selected: 1, deep_link: "" });
      return [
        g(1, "year", "2026 备考期", "2026-08-01..2027-08-01"),
        g(2, "month", "2026年8月", "2026-08"),
        k(1, "高数"), k(2, "数据结构"), k(3, "英语 / 词汇积累"),
        g(3, "day", "8月18日学习", "2026-08-18"),
        t(1, "高数：极限计算基础题 15题", "2026-08-18", 60, "structured"),
        t(2, "英语：词汇复习 30min", "2026-08-18", 30, "accumulation"),
        g(4, "day", "8月19日学习", "2026-08-19"),
        t(3, "数据结构：线性表基本概念 + 10道基础题", "2026-08-19", 90, "structured"),
        g(5, "day", "8月20日休息", "2026-08-20", true),
      ];
    },
    set_ai_change_op_selected: () => null,
    apply_ai_change_set: () => null,
    reject_ai_change_set: () => null,
    undo_ai_change_set: () => null,
    open_external_url: () => null,
    save_final_goal_brief: () => null,
    // DEV-0058：Final Goal 卡 mock（缺项文案与后端新文案一致——无内部字段名）
    get_final_goal_state: () => ({
      brief: { title: "2027 研究生考试", outcome: "初试总分 380+，一志愿上岸", deadline: "2027-12-25", success_criteria: ["初试过线"], scope: [], constraints: [], unresolved: [] },
      conflicts: [],
      missing: [],
    }),
    get_learning_totals: () => ({ learning_days: 3, total_seconds: 12600, daily_avg_minutes: 70, today_seconds: 3600, today_tasks_total: 5, today_tasks_completed: 3, needs_review_count: 0 }),
    get_knowledge_time_distribution: () => [[{ name: "数学", seconds: 9000, item_id: 1, child_count: 2 }, { name: "英语", seconds: 3600, item_id: 2, child_count: 1 }], 0],
    get_time_of_day_distribution: () => [["06-09", 1800], ["09-12", 5400], ["12-15", 3600], ["15-18", 0], ["18-21", 1800], ["21-24", 0], ["00-06", 0]],
    get_plan_vs_actual: () => [["2026-08-16", 180, 60, 3, 1], ["2026-08-17", 120, 90, 2, 2]],
  };

  // Tauri shim：补 @tauri-apps/api/event 需要的 transformCallback + 标准 unlisten 结构
  let cbSeq = 0;
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      const h = handlers[cmd];
      if (h) return h(args ?? {});
      if (cmd === "plugin:event|listen" || cmd === "plugin:event|unlisten") return null;
      console.warn("[mock] unhandled invoke:", cmd);
      return null;
    },
    transformCallback: (cb) => {
      const id = ++cbSeq;
      window["_" + id] = cb;
      return id;
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    plugins: { event: { listen: async () => ({ unregisterListener: () => {} }), unlisten: async () => {} } },
  };
  console.log("[mock] Tauri browser shim injected (DEV-0055 smoke)");
})();
