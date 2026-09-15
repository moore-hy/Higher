// Foundation 2.0 §6: IPC command boundary.
//
// Tauri command handlers are grouped by domain here. Per taskbook §6, the
// command layer must NOT contain large business algorithms — those live in
// `repository`, `domain`, `agent`, `search`, `recurrence`, and `platform`.
// Existing Tauri command *names* are preserved so the front-end is unaffected.
pub mod agent;
pub mod companion;
pub mod data;
pub mod intake;
pub mod knowledge;
pub mod learning_state;
pub mod planning;
pub mod profile;
pub mod recurrence;
pub mod settings;
pub mod sync;
pub mod system;
