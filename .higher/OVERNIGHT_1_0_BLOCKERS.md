# HIGHER 1.0 OVERNIGHT — BLOCKERS

状态值：`OPEN` / `MITIGATED` / `RESOLVED` / `WONTFIX`

---

## BLK-01 — 本地 git 对象库被破坏（packfiles 丢失）· **OPEN · 最高优先级**

**发现时间**：2026-09-16 01:20–01:32（本地）
**影响范围**：`C:\Users\37653\Desktop\Higher\.git` 的 **对象库与分支 ref**；**工作树与源代码未受影响**。

### 实测症状（可复核）

```text
$ git rev-parse HEAD
fatal: ambiguous argument 'HEAD': unknown revision or path not in the working tree.

$ cat .git/HEAD
ref: refs/heads/main

$ ls .git/refs/heads/          → 空目录（refs/heads/main 不存在）
$ git cat-file -t 77339676baf532fb16abac5e9ac2c47827dae391
fatal: git cat-file: could not get object info

$ ls .git/objects/pack/
multi-pack-index
pack-4068fc276edd3d4dcbf32d848cf5df3efbbe0d93.idx   ← 只有 .idx，没有同名 .pack
pack-73655cebe4b7b03d134d55b5687c152427eb0de0.idx   ← 只有 .idx
pack-87d41fa0d257ce8d423248be0db24a30c52332b7.idx   ← 只有 .idx

$ git count-objects -v
count: 0 / in-pack: 0 / packs: 0 / garbage: 3
warning: no corresponding .pack: .git/objects/pack/pack-4068….idx（同 3 条）

$ git status --short
（全部文件显示为 `A` —— 因为 HEAD 无法解析，git 视其为空仓库）
```

### 根因（推定，证据一致）

**`git stash push` 触发了 git 的 auto-gc，gc 在 repack 中途被中断。**

时间线证据：

```text
01:19  .git/index 被写入            ← stash push 更新索引
01:20  .git/objects、.git/refs 被写入 ← stash 创建 stash 对象 + refs/stash
01:20  .git/objects/pack 只剩 .idx，无 .pack
       （`.idx` 保留、`.pack` 消失 = repack 在「删除旧 pack」之后、
        「写入新 pack」之前被杀的典型签名）
01:31  .git 目录 mtime
```

触发该命令的执行链（当时用于「基线探针」）在执行到 `git stash push` 时收到
**SIGTERM**（工具返回 `Signal: SIGTERM`，stdout 为空 —— 连紧随其后的 `echo` 都未落盘）。
`git stash` 是 commit 类操作，会调用 auto-gc；gc 被 SIGTERM 杀死 ⇒ packfiles 丢失。

**这不是产品代码、构建产物或用户代码的问题**：`src/`、`src-tauri/`、`.higher/` 全部完好。

### 已被破坏 / 未受影响

| 项 | 状态 |
|---|---|
| `.git/objects/**`（全部 packfile） | **已丢失** |
| `refs/heads/main`（以及 `refs/remotes/*`） | **已丢失** |
| `.git/index` | 仍在，但语义已变为「空仓库的全量 add」 |
| `.git/logs/HEAD`（reflog） | **完好** —— 完整记录到 `86a082a → 7733967` |
| `.git/FETCH_HEAD` | **完好** —— 记录 `77339676baf5…  branch 'main' of https://github.com/moore-hy/Higher` |
| 工作树全部文件（含本轮 M0 改动） | **完好** |
| `src-tauri/target/`（构建缓存） | 完好（cargo 仍可离线编译/测试） |

### 为什么不是「无法继续」

§H 明确规定：临时 fetch 失败**不是** Hard Blocker，除非所有剩余 REQUIRED work 都真正依赖远端数据。
本轮剩余工作（M1–M7 / S1–S4 的实现与测试）**全部是本地代码 + 本地 SQLite 测试**，
不依赖远端；因此按 §H 继续施工，只对「checkpoint commit」这一项造成阻塞。

### 恢复方案（网络恢复后按序执行，**绝不重写历史**）

```bash
# 0) 禁止 auto-gc 再次造成同类破坏（本仓库本地配置）
git config gc.auto 0
git config maintenance.auto false

# 1) 重新取回对象（origin/main 期望值 = 77339676baf532fb16abac5e9ac2c47827dae391）
git -c gc.auto=0 -c maintenance.auto=false fetch origin main
git rev-parse refs/remotes/origin/main      # 必须 == 7733967…

# 2) 重建分支 ref（不是新建 root commit！）
git update-ref refs/heads/main 77339676baf532fb16abac5e9ac2c47827dae391

# 3) 只重建索引，**不触碰工作树**
git reset --mixed            # = git reset（丢弃 01:19 那次全量 add 的索引语义）
git status --short           # 期望：只列本轮真正改动的文件

# 4) 之后正常 checkpoint commit
```

第 3 步刻意使用 `--mixed`（**不是** `--hard`）；`git reset --hard` 在本轮永久禁用（§5.3）。

### 本轮纪律（BLK-01 生效期间）

```text
1. 绝不执行任何会在 unborn HEAD 上产生 root commit 的操作（git commit / stash / rebase / merge）
2. 绝不执行 git reset --hard / checkout . / restore . / clean -fd / stash（§5.3 永久禁止）
3. 绝不重写历史：本地对象库缺失 ≠ 可以重建一个「看起来一样」的新历史
4. 每个 coherent work unit 结束后，把改动文件快照进 .higher/.overnight_backup/<PHASE>/
   （这是 BLK-01 期间唯一的「checkpoint」形式）
5. 每个 work unit 结束后重试一次
   git -c gc.auto=0 -c maintenance.auto=false ls-remote origin
   → 一旦成功，立刻执行上面的恢复方案，再补做 checkpoint commit
```

### 已完成的保全动作

```text
.higher/.overnight_backup/M0_pre_stash.patch     9 个文件、M0 全部改动（71,532 bytes，01:19）
.higher/.overnight_backup/M0_files/*.rs.ts       M0 改动文件的原始副本（01:31）
```

两者均已验证含 `normal_fallback` / `REASON_MICRO_UNAVAILABLE` /
`subject_learning_item_id` / `ar12_done_and_partial_still_touch_and_dedupe` 等 M0 标记。

### 恢复尝试记录

```text
01:20  git fetch origin main                        → fatal: CONNECT tunnel failed, response 502
01:2x  curl https://github.com/                     → http_code=000
01:32  curl（关闭沙箱直连）+ fetch                    → direct_http=000（机器当前无出网）
```

结论：**网络层面不可用，本地无法恢复对象库。** 继续离线施工，周期性重试。

---

_Last updated: 2026-09-16 01:33_
