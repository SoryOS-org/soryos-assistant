use super::super::mcp::{McpAction, McpActionResult, McpActionStatus, McpPlan};
use super::engine::ensure_step_index;
use super::history::planner_status_label;
use serde_json::json;

#[test]
fn action_arguments_default_object() {
    let action = McpAction {
        server_id: "demo".into(),
        tool_name: "echo".into(),
        arguments: serde_json::Value::Null,
        confirmation_required: false,
        timeout_ms: None,
        step_index: None,
        status_note: None,
    };

    let value = action.arguments_or_default();
    assert_eq!(value, json!({}));
}

#[test]
fn plan_optional_fields_deserialize() {
    let value = json!({
        "session_token": "abc123",
        "actions": [
            {
                "server_id": "demo",
                "tool_name": "echo"
            }
        ]
    });

    let plan: McpPlan = serde_json::from_value(value).expect("plan decode");
    assert_eq!(plan.session_token.as_deref(), Some("abc123"));
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].tool_name, "echo");
}

#[test]
fn action_result_helpers_work() {
    let success = McpActionResult::completed(0, json!({"ok": true}), Some(42));
    assert_eq!(success.step_index, 0);
    assert!(matches!(success.status, McpActionStatus::Completed));
    assert_eq!(success.output.expect("output"), json!({"ok": true}));
    assert_eq!(success.latency_ms, Some(42));

    let failure = McpActionResult::transport_error(1, "boom");
    assert_eq!(failure.step_index, 1);
    assert!(matches!(failure.status, McpActionStatus::TransportError));
    assert_eq!(failure.error.as_deref(), Some("boom"));
    assert!(failure.output.is_none());
}

#[test]
fn ensure_step_index_respects_provided_value() {
    let mut counter = 3;
    let value = ensure_step_index(&mut counter, Some(5));
    assert_eq!(value, 5);
    assert_eq!(counter, 6);

    let value_two = ensure_step_index(&mut counter, None);
    assert_eq!(value_two, 6);
    assert_eq!(counter, 7);
}

#[test]
fn ensure_step_index_prevents_reuse_when_plan_rewinds() {
    let mut counter = 4;
    let provided = ensure_step_index(&mut counter, Some(1));
    assert_eq!(provided, 4);
    assert_eq!(counter, 5);

    let next = ensure_step_index(&mut counter, None);
    assert_eq!(next, 5);
    assert_eq!(counter, 6);
}

#[test]
fn planner_status_label_matches_variants() {
    assert_eq!(
        planner_status_label(&McpActionStatus::Completed),
        "completed"
    );
    assert_eq!(
        planner_status_label(&McpActionStatus::TransportError),
        "transport_error"
    );
    assert_eq!(planner_status_label(&McpActionStatus::Skipped), "skipped");
    assert_eq!(planner_status_label(&McpActionStatus::Timeout), "timeout");
    assert_eq!(
        planner_status_label(&McpActionStatus::Cancelled),
        "cancelled"
    );
    assert_eq!(planner_status_label(&McpActionStatus::Retry), "retry");
}
