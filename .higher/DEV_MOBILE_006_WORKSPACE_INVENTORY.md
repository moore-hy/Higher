# DEV-MOBILE-006 · Workspace Inventory（阶段一：只审计）

- 日期：2026-08-28
- 范围：`C:\Users\37653\Desktop\Higher\Higher-Android`（分支 `android/dev`）

## 总量（Before，清理执行前实测）

| 范围 | 文件数 | 大小 |
|---|---|---|
| repo（不含 node_modules / .toolchain） | 33,950 | **33,436.2 MB** |
| node_modules（保留） | 17,779 | 209.1 MB |
| .toolchain（保留） | 491 | 523.5 MB |

## 逐目录审计（含 git 状态与可再生性判定）

| 路径 | 文件 | MB | 最后修改 | git | 再生方式 | 判定 |
|---|---|---|---|---|---|---|
| `dist/` | 15 | 3.0 | 08-28 13:38 | ignored | vite（Build Step 9） | A 级可删 |
| `.mobile-test-build/` | 8 | 0.0 | 08-28 13:31 | untracked | tsc（test:mobile） | A 级可删 |
| `.ai-runtime-test-build/` | 2 | 0.0 | 08-28 13:31 | ignored | tsc（test:ai-runtime） | A 级可删 |
| `.higher/tmp/` | 4 | 0.0 | 08-28 14:33 | untracked | —（临时） | A 级可删（内容级） |
| `src-tauri/target/` | 31,435 | **31,797.7** | 08-28 13:39 | ignored | cargo（全量重编很慢） | 仅 `-Deep` 人工选项 |
| `src-tauri/gen/android/.gradle/` | 16 | 7.6 | 08-28 13:40 | ignored | gradle | A 级可删 |
| `src-tauri/gen/android/build/` | 1 | 0.1 | 08-28 13:40 | ignored | gradle | A 级可删 |
| `src-tauri/gen/android/buildSrc/build/` | 114 | 0.6 | 08-27 18:24 | ignored | gradle/kotlin | A 级可删 |
| `src-tauri/gen/android/buildSrc/.gradle/` | 7 | 0.1 | 08-28 13:40 | ignored | gradle | A 级可删 |
| `src-tauri/gen/android/buildSrc/.kotlin/` | 1 | 0.0 | 08-27 18:24 | untracked | kotlin | A 级可删 |
| `src-tauri/gen/android/app/build/` | 1,756 | **1,305.4** | 08-28 13:40 | ignored | gradle (AGP) | A 级可删 |
| `gen/.../jniLibs/**/*.so` | 2 | 38.8 | 08-28 13:39 | untracked（tracked=0 实测） | Build Step 12（cargo target 复制） | 仅删 *.so，保留目录 |
| `gen/.../app/src/main/assets/` | 15 | 3.0 | 08-28 13:38 | untracked | Build Step 13（dist mirror） | 已证明 100% generated，可清 |
| `release/android/` | 15 | 267.3 | 08-28 14:30 | ignored | Build/Promote | 收口：仅留 Stable 三件套 |

## 关键证明（解锁删除的前提）

1. **Stable 已 Promotion**：`Higher-v0.1.0.apk`（42.5 MB）SHA `d60326ae575601666317993a8ac1a30afb8fec6d384eb4d168664423101b3e1d` == RC SHA（字节级一致）。
2. **assets 100% mirror**：assets 15 文件集合 ⊆ dist 15 文件集合，逐项比对无人工资源（无 STOP 项）。
3. **jniLibs `*.so` 零 git track**（`git ls-files` 实测 0）。
4. **仓库根 `0` 文件零引用**：`src/`、`scripts/`、`src-tauri/`（rs/toml/json/kt/kts/gradle）、`tests/` 全量搜索文件引用形态（`./0`、`'0'`、`"0"` 路径用法）——唯一命中为 `padStart(2, "0")` 类字符串字面量，与文件无关。判定：历史命令误生成垃圾。

## 保留（非垃圾，不动）

`node_modules/`（开发需要）· `.toolchain/`（canonical build 用便携 JDK）· `src-tauri/target/`（默认保留，`-Deep` 才清）· `.higher/archive/` 与 `.higher/*.md`（历史设计/任务/报告）· 全部红线目录（见 DEV_MOBILE_006_ANDROID_WORKSPACE_HYGIENE_REPORT.md §红线对照）。
