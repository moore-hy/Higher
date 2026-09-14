// Foundation 2.0 §7: Rust → TypeScript DTO single source of truth (ts-rs 12.0.1).
//
// Rules (taskbook §7):
// - Rust structs are the ONLY authoritative source of cross-IPC types.
// - Only DTOs that actually cross the IPC boundary are annotated with
//   `#[derive(ts_rs::TS)]`; DB-only structs stay unannotated.
// - `i64` IDs are exported as TS `number` (see `with_large_int("number")`
//   in the export test below) — never `bigint`.
// - Time fields stay plain `string`; never silently become `Date`.
//
// Generation: `npm run generate:types` (runs `cargo test export_ipc_dtos`
// inside src-tauri). Files land in `src/generated/`.
pub mod dto;
