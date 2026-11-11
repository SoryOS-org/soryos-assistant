pub mod assistant;
pub mod logs;
pub mod machine;
pub mod mcp;
pub mod metrics;
pub mod panel;
pub mod preferences;
pub mod secrets;
pub mod settings;
pub mod stt;
pub mod system;

pub use assistant::{
    get_application_catalog, handle_intent, handle_intents_batch, process_user_request,
    refresh_application_catalog,
};
pub use logs::get_recent_logs;
pub use machine::{get_conversation_memory, reset_worker_session};
pub use mcp::{get_mcp_registry, open_mcp_config_file, toggle_mcp_server};
pub use metrics::get_latency_hud;
pub use panel::{
    close_window, get_ui_snapshot, panel_hide, panel_navigate, set_sprite_preload_state,
};
pub use preferences::{get_preferences_snapshot, update_preferences};
pub use secrets::{
    clear_picovoice_key, clear_worker_auth_token, get_secrets_status, set_picovoice_key,
    set_worker_auth_token,
};
pub use settings::{get_config_summary, update_config};
pub use stt::{get_preload_status, toggle_ptt};
pub use system::get_about_metadata;
