//! Machine state maintenance commands exposed to the panel UI.

use crate::config::machine_state::MachineStateManager;
use tauri::State;

/// Reset the worker-side conversation history and response handles.
#[tauri::command]
pub async fn reset_worker_session(
    machine_state: State<'_, MachineStateManager>,
) -> Result<(), String> {
    machine_state
        .reset_session()
        .await
        .map_err(|err| err.to_string())
}

/// Fetch stored conversation memory for hydrating the chat UI.
#[tauri::command]
pub async fn get_conversation_memory(
    machine_state: State<'_, MachineStateManager>,
) -> Result<crate::config::machine_state::ConversationMemory, String> {
    machine_state
        .fetch_memory()
        .await
        .map_err(|err| err.to_string())
}
