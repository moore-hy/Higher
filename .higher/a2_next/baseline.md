# BASELINE GATE（W0 / §5）

**Branch:** `main`
**HEAD:** `96cc109f6b2fc33faee5b6ea7d378670bed7be45`（与 taskbook 预期 baseline **一致**）
**Working tree:** 干净（只有 untracked 的非任务目录，无 task-owned production 文件被改动）

## 命令与结果

```bash
# targeted
cargo test --test a2_1_personal_evidence_authority -j 1     → 44 passed / 0 failed
cargo test --test learner_model_v2 -j 1                     →  9 passed / 0 failed
cargo check --lib -j 1                                      → ok
npx tsc --noEmit                                            → ok

# broad
cargo test --no-fail-fast -j 1 -- --test-threads=1
```

## Broad baseline 结果

```text
tests executed : 2055
passed         : 1898
FAILED         : 29   ← BASELINE_FAILURE_SET
```

日志：`.higher/a2_next/baseline_broad.log`
清单：`.higher/a2_next/baseline_failures.txt`

## BASELINE_FAILURE_SET（29）

```text
boot_tc001_db_ready_before_webview
de024_forward_migration_from_old_schema_preserves_data
de025_micro_migration_only_adds_one_table
f1_tc004_legacy_path_never_produces_active
f2_tc005_search_higher_shares_repository_gate
governance_production_call_graph
r21_ai_runs_running_first
r23_terminal_updates_same_row
r25_legacy_readonly_conversation_not_blocked
r26_limited_but_control_compatible_action_allowed
r41_control_temperature_zero
r42_provider_budget
rt_gr_01_real_pdf_reaches_real_grounded_material
rt_gr_02_real_grounded_material_round_trips_on_a_real_training_run
runtime_tc015_governance_no_direct_emit_in_production_ai_modules
rw09_watermark_above_today_clamps_to_zero
t12_ai_only_two_modes
t14_whole_rail_clickable
t19_single_nonstream_fallback
t21_control_incompatible_safe_reject
t22_primary_tools_guard
t54_complete_request_no_history
t55_reference_request_allows_history
t57_no_fake_proposal_prose
tc016_ai_runtime_semantics_frozen
test_runtime_db_path_no_hardcoded_manifest_dir_only
u12_current_session_continue_end_handlers_exist
u21_ai_panel_collapsed_mode
u26_no_new_important
```

> 这些是**基线已有**的红（多为 AI runtime / migration / docling 网络相关）。
> 按 taskbook §2，它们**不是** Hard Blocker，也不要求本轮修好。
> 唯一要求：`FINAL_FAILURE_SET - BASELINE_FAILURE_SET = ∅`。

## fmt 基线

`cargo fmt --check` 的既有欠债为 **4 个文件**（与项目长期记录一致）：

```text
src\ai\secret_migration.rs
src\commands\agent.rs
src\repository\search.rs
tests\secret_store_cutover.rs
```

本轮**不得**新增欠债（清理只对自己改动的文件用 `rustfmt --edition 2021 <file>`）。
