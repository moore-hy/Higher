// Foundation 2.0 §6: knowledge-domain commands (KnowledgeDocument / workspace / move).
use crate::db;
use crate::repository::learning_item::LearningItemRepository;

// =============== Knowledge Move（DEV-0028） ===============

/// 移动知识节点（拒绝：自己/后代/跨 Goal/非法 parent）。
#[tauri::command]
pub fn move_learning_item(
    state: tauri::State<'_, db::DbState>,
    id: i64,
    new_parent_id: Option<i64>,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    LearningItemRepository::new(&conn).move_item(id, new_parent_id)
}

