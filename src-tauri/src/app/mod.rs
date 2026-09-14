// Foundation 2.0 §6: application assembly (Tauri Builder + startup lifecycle).
//
// lib.rs keeps only the mobile entry-point anchor (`run`); the Builder chain
// (plugins + command registration) lives in `builder.rs` and the `.setup()`
// startup lifecycle lives in `lifecycle.rs`.
pub mod builder;
pub mod lifecycle;
