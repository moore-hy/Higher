# HIGHER PRODUCT 2.0 · OVERNIGHT PROGRESS

> 对应任务书：`HIGHER_PRODUCT_2_OVERNIGHT_MASTER_TASK_V2_3_BALANCED.md`
> 状态：**IN PROGRESS — MORNING_READY = NOT YET PASS**

---

## 0. Migration Number Ledger（§0C.3 · 冻结点）

执行前已读取仓库实际最高 migration：**`v029_local_sync_backfill_outbox.rs`**（`src-tauri/src/migrations/` 共 29 个 vNNN + `mod.rs`）。

```text
NEXT_MIGRATION_AT_START = v029 (已有最高)
预留槽位（唯一，不允许两模块共用）：

v030  Secrets          — Foundation D（Native SecretStore）
v031  Recurrence       — Foundation H（RFC5545 / rrule）
v032  SearchV2         — Foundation J（FTS5 + jieba）
v033  KnowledgeCanvas  — §35（knowledge_canvases + knowledge_canvas_embeds）
v034  PlanningIntake   — §24.3（planning_intake_drafts）
v035  LearningSignals  — §30A.1（learning_signals）
v036  KnowledgeMastery — §30A.2（knowledge_mastery）
v037+ 顺延，实际新增前必须回写本 Ledger
```

**本文件为唯一 Ledger。任何新增 migration 前先在此登记。**

---

## 1. Git

| 项 | 值 |
|---|---|
| branch | `main` |
| base（任务书基线） | `7874fd42d437b1b72d76222dcb5129f035d97a08` |
| HEAD（开工时） | `7874fd4 refactor: continue Higher Foundation 2.0` |
| worktree（开工时） | clean |
| push / tag / release | **NO** |

开工 precheck（§0.1）：`branch=main` ✓、HEAD == 基线 SHA ✓、无未提交改动 ✓。

---

## 2. 环境约束（沿用上次会话实测结论，未变）

| 约束 | 应对 |
|---|---|
| bash PATH 为 Windows 分号格式，coreutils 找不到 | 每条 bash 命令前置 `export PATH=/usr/bin:/bin:/c/Windows/System32:$PATH` |
| PowerShell 工具不回传 stdout | 一律用 bash；必须用 PowerShell 时把输出重定向到文件再 Read |
| `.git/refs/remotes/**` 不可写 | 远端核对以 SHA 代替 `origin/main` |
| `npm ci` 卡死 | 用 `npm install --prefer-offline` |
| CRLF stat 脏（`core.autocrlf=true`，无 `.gitattributes`） | `src/generated` 出现假 M 时以 `git diff` 为准 |
| Smart App Control 可能拦截 Cargo 测试 exe | 以 `cargo check` + `cargo test --no-run` 为默认验证 |
| 单次重活耗时 | `cargo check` ~6min、`cargo test` ~15min、`npm ci` 30min+ |

---

## 3. WAVE 0 — Safety Harness

| 项 | 状态 |
|---|---|
| 0.1 main / HEAD / worktree precheck | **DONE** |
| §0.3 `.workbuddy` gitignore + `git rm --cached` | **DONE** |
| 0C.3 Migration Ledger | **DONE**（见 §0） |
| 0.4 AppErrorBoundary | see below |
| 0.5 Route fallback | see below |
| 0.6 product UI test harness | see below |
| 0.8 package scripts | see below |

---

## 4. 下一步（Next exact action）

见本文件末尾滚动更新。
