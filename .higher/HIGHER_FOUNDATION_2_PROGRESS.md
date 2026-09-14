# HIGHER-FOUNDATION-2.0 · 执行进度报告

> 对应任务书：`HIGHER_FOUNDATION_2_CLOSURE_MASTER_TASK.md`
> 报告时间（SYSTEM）：2026-09-14T23:5x+0800
> **状态：IN PROGRESS — Foundation 2.0 = NOT COMPLETE**

---

## 1. Git

| 项 | 值 |
|---|---|
| branch | `main` |
| base（任务书基线） | `0aa69b3e7125aed99f5e21857baf34c0cbdf65fc` |
| current HEAD | `b8e28aa chore(rust): apply rustfmt baseline to repository` |
| worktree | clean（仅 `.workbuddy/` 未跟踪，为工具内部记忆目录） |
| pushed | **NO** |
| tagged | **NO** |
| released | **NO** |

远端 `refs/heads/main` == `0aa69b3e…`，与任务书基线一致（已 `git ls-remote` 验证）。

---

## 2. Phase A — Baseline Gate

| Gate | 命令 | 结果 |
|---|---|---|
| BASELINE_TSC | `npx tsc --noEmit` | **PASS** |
| BASELINE_FRONTEND_BUILD | `npm run build` | **PASS** |
| BASELINE_GENERATED_TYPES | `git diff --exit-code src/generated` | **PASS**（exit 0，ts-rs 生成幂等） |
| BASELINE_CARGO_FMT | `cargo fmt --check` | **FAIL → 已按授权修复 → PASS** |
| BASELINE_CARGO_CHECK | `cargo check -j 1` | **PASS**（6m08s，34 warnings） |
| BASELINE_IPC_TEST | `cargo test -j 1 ipc::dto` | **PASS**（2 passed） |

### Phase A 实际修复的两个真实缺口

1. **声明但未安装的依赖**：`@tanstack/react-query@5.102.8`、`temporal-polyfill@1.0.5` 只在 `package.json`/`package-lock.json` 中声明，`node_modules` 中缺失（`npm ls` 报 UNMET DEPENDENCY）。Phase B 无法启动。**已安装并验证可解析。**
2. **`cargo fmt --check` 从未通过**：仓库全程 2 空格缩进、无 `rustfmt.toml`、`cargo fmt` 不在任何 CI（`.github/workflows/windows-release.yml` 无 fmt 步骤）。实测配置调优无效（差异反升至 238 文件）→ 经 Owner 授权一次性全仓格式化：201 个 `.rs` 文件，第二次运行后 `--check` 收敛为 0 差异，`cargo check` 仍 PASS。

---

## 3. Phase B — TanStack Query（进行中，未提交）

已完成（§7.1 部分 / §7.2 / §7.3）：

- 新增 `src/query/client.ts` — 单例 QueryClient（query retry=1、mutation retry=0、staleTime 30s、`refetchInterval=false`）。
- 新增 `src/query/keys.ts` — key 单一事实源，覆盖 §7.3 全部要求键 + `learningItems`；所有 profile-scoped key 均含 `profileId`。
- `src/main.tsx` — 挂载 `<QueryClientProvider client={queryClient}>`。
- 验证：`npx tsc --noEmit` PASS、`npm run build` PASS。

未完成（Phase B 剩余）：

- domain hooks 模块：`profiles/goals/tasks/sessions/planning/knowledge/data/settings/sync`（`src/api.ts` 共 286 个导出需逐一归类）。
- 14 个页面从 `useEffect → setLoading → Promise.all → setX` 迁移到 `useQuery`/`useMutation`：Today、Planning、Knowledge、Data、Settings、Sync、ProfileSelector、Tasks、Goals、Evaluations、History、LearningWorkspace、Progress、Review。
- `ActiveProfileContext` 删除 `refreshKey` / `triggerRefresh`（当前 **51 处引用 / 18 个文件**），保留 bounded bootstrap（timeout / 2 attempts / error phase）。
- `sync://completed`、`ai://applied` → 精准 invalidate（替换 refreshKey 驱动）。

**§7.8 完成标准当前未满足**：`refreshKey production usage = 0` 尚未达成。

---

## 4. 环境约束（已实测，需架构方知悉）

| 约束 | 现象 | 影响 / 应对 |
|---|---|---|
| `.git/refs/remotes/**` 不可写 | `git fetch` 只能写 `FETCH_HEAD`；`update-ref` 返回 0 但文件未落盘 | `origin/main` ref 无法建立 → 最终审计以 SHA `0aa69b3` 代替 `origin/main` |
| `npm ci` 卡死 | 30 分钟停在 `idealTree buildDeps`，npm cache 与文件计数静止 | 改用 `npm install --prefer-offline` 恢复；已记录 |
| CRLF stat 脏 | `core.autocrlf=true`、无 `.gitattributes`；ts-rs 写 LF | 跑完 `generate:types` 后 `git status` 显示 18 个 M，但 `git diff` 为空；`git checkout -- src/generated` 恢复 |
| bash PATH 被污染 | PATH 为 Windows 分号格式，coreutils 找不到；PowerShell 工具不回传 stdout | 统一用 bash + 命令内显式重设 PATH |
| Smart App Control | `.higher/WORKING_RULES.md` 警告 SAC 可能拦截 Cargo 测试 exe（4551） | 本次 `cargo test ipc::dto` 未被拦截；仍按规则以 `cargo check` + `cargo test --no-run` 为默认验证 |
| 单次重活耗时 | `cargo check` 首次 6m08s、`cargo test` 15m46s、`npm ci` 30min+ | Phase A→N 全量在一个会话内不可完成 |

---

## 5. 规模事实（用于评估剩余工作量）

- 前端：`src/api.ts` 286 导出 / 78 KB；`src/types.ts` 33 KB；`Settings.tsx` ≈115 KB；`Knowledge.tsx` ≈80 KB。
- Rust：`src-tauri/src/**` 164 个 `.rs`；集成测试 81 个文件。
- 任务书要求新增：迁移 v030/v031/v032、`agent/` 全量 Rig 运行时（含 AGENT2-TC001–022）、`search/`、`recurrence/`、`platform/secrets/`、`platform/reminders/`、`sync/` 重构、Updater + CI workflow、前端 Settings/Knowledge 拆分。

---

## 6. 结论

严格按任务书 §37：**只要 Definition of Done 未全部满足，Foundation 2.0 = NOT COMPLETE。**

当前：Phase A 完成并已提交；Phase B 进行中（约 20%）。Phase C→N 未开始。
