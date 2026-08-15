# Higher 开发环境摘要

> 详细技术实况：`../ENVIRONMENT.md`（本文件仅速查）

- OS：Windows 11（10.0.26200）
- Node：v24.18.0 / npm 11.16.0
- Rust：1.97.1（cargo 需手动加 PATH）
- Tauri：crate 2.11.3 / CLI 2.11.4
- Schema：v013
- DB：dev `src-tauri/.data/higher.db`；prod `%LOCALAPPDATA%\com.higher.desktop\higher.db`
- 附件：dev `src-tauri/.data/attachments/`；prod `app_data_dir/attachments/`
- Last Gate：2026-08-16 — TS 0 错误 / check 0 / test 204 通过 0 失败 0 ENV_BLOCKED / tauri dev `latest v013` 正常
