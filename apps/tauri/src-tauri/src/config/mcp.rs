use anyhow::{Context, Result};
use mcp_host::{
    config::{HeartbeatConfig, TransportTimeouts},
    HostConfig, PolicyConfig, ServerConfig, TransportConfig,
};
use serde_json::{json, Map, Value};
use tauri::{App, AppHandle};

use super::{defaults, io};

const CONFIG_FILENAME: &str = "mcp.config.json";

pub type McpConfig = HostConfig;

pub fn load(app: &App) -> Result<McpConfig> {
    let cfg = io::load_or_create(
        app,
        CONFIG_FILENAME,
        |content| serde_json::from_str::<McpConfig>(content).context("Failed to parse MCP config"),
        |config| {
            let serialized = serialize_mcp_config(config);
            serde_json::to_string_pretty(&serialized)
                .context("Failed to serialise MCP configuration")
        },
        || apply_policy_defaults(McpConfig::default()),
    )?;
    Ok(apply_policy_defaults(cfg))
}

pub fn save_to_handle(app_handle: &AppHandle, config: &McpConfig) -> Result<()> {
    io::save_to_handle(app_handle, CONFIG_FILENAME, config, |value| {
        let serialized = serialize_mcp_config(value);
        serde_json::to_string_pretty(&serialized).context("Failed to serialise MCP configuration")
    })
}

fn serialize_mcp_config(config: &McpConfig) -> Value {
    let mut root = Map::new();
    let mut servers = Map::new();
    for (id, server) in &config.servers {
        servers.insert(id.clone(), server_to_value(server));
    }
    root.insert("mcpServers".into(), Value::Object(servers));

    if !is_default_policy(&config.policy) {
        root.insert("policy".into(), policy_to_value(&config.policy));
    }

    Value::Object(root)
}

pub fn parse_config(content: &str) -> Result<McpConfig> {
    let config: McpConfig = serde_json::from_str(content).context("Failed to parse MCP config")?;
    Ok(apply_policy_defaults(config))
}

pub fn config_file_fingerprint(config: &McpConfig) -> Option<String> {
    let serialized = serialize_mcp_config(config);
    serde_json::to_string(&serialized).ok()
}

fn server_to_value(server: &ServerConfig) -> Value {
    let mut map = Map::new();

    if let Some(name) = &server.display_name {
        map.insert("display_name".into(), Value::String(name.clone()));
    }

    if !server.enabled {
        map.insert("enabled".into(), Value::Bool(false));
    }

    apply_transport_fields(&mut map, &server.transport);

    if !is_default_policy(&server.policy) {
        map.insert("policy".into(), policy_to_value(&server.policy));
    }

    if let Some(template) = &server.prepare {
        let mut prepare = Map::new();
        prepare.insert("command".into(), Value::String(template.command.clone()));
        if !template.args.is_empty() {
            prepare.insert(
                "args".into(),
                Value::Array(
                    template
                        .args
                        .iter()
                        .map(|arg| Value::String(arg.clone()))
                        .collect(),
                ),
            );
        }
        if !template.env.is_empty() {
            prepare.insert("env".into(), map_string_map(&template.env));
        }
        if let Some(dir) = &template.current_dir {
            prepare.insert("current_dir".into(), Value::String(dir.clone()));
        }
        map.insert("prepare".into(), Value::Object(prepare));
    }

    if !server.metadata.is_empty() {
        map.insert(
            "metadata".into(),
            Value::Object(
                server
                    .metadata
                    .clone()
                    .into_iter()
                    .collect::<Map<String, Value>>(),
            ),
        );
    }

    Value::Object(map)
}

fn apply_transport_fields(target: &mut Map<String, Value>, transport: &TransportConfig) {
    match transport {
        TransportConfig::Stdio {
            command,
            args,
            env,
            current_dir,
            timeouts,
            heartbeat,
        } => {
            target.insert("type".into(), Value::String("stdio".into()));
            target.insert("command".into(), Value::String(command.clone()));
            if !args.is_empty() {
                target.insert(
                    "args".into(),
                    Value::Array(
                        args.iter()
                            .map(|value| Value::String(value.clone()))
                            .collect(),
                    ),
                );
            }
            if !env.is_empty() {
                target.insert("env".into(), map_string_map(env));
            }
            if let Some(dir) = current_dir {
                target.insert("current_dir".into(), Value::String(dir.clone()));
            }
            if !is_default_timeouts(timeouts) {
                target.insert("timeouts".into(), timeouts_to_value(timeouts));
            }
            if let Some(heartbeat) = heartbeat {
                target.insert("heartbeat".into(), heartbeat_to_value(heartbeat));
            }
        }
        TransportConfig::Ws {
            url,
            headers,
            timeouts,
            heartbeat,
        } => {
            target.insert("type".into(), Value::String("ws".into()));
            target.insert("url".into(), Value::String(url.clone()));
            if !headers.is_empty() {
                target.insert("headers".into(), map_string_map(headers));
            }
            if !is_default_timeouts(timeouts) {
                target.insert("timeouts".into(), timeouts_to_value(timeouts));
            }
            if let Some(heartbeat) = heartbeat {
                target.insert("heartbeat".into(), heartbeat_to_value(heartbeat));
            }
        }
        TransportConfig::Http {
            url,
            headers,
            timeouts,
            heartbeat,
        } => {
            target.insert("type".into(), Value::String("http".into()));
            target.insert("url".into(), Value::String(url.clone()));
            if !headers.is_empty() {
                target.insert("headers".into(), map_string_map(headers));
            }
            if !is_default_timeouts(timeouts) {
                target.insert("timeouts".into(), timeouts_to_value(timeouts));
            }
            if let Some(heartbeat) = heartbeat {
                target.insert("heartbeat".into(), heartbeat_to_value(heartbeat));
            }
        }
    }
}

fn map_string_map(map: &std::collections::BTreeMap<String, String>) -> Value {
    Value::Object(
        map.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

fn policy_to_value(policy: &PolicyConfig) -> Value {
    let mut map = Map::new();
    map.insert(
        "handshake_timeout_ms".into(),
        Value::Number(policy.handshake_timeout_ms.into()),
    );
    map.insert(
        "call_timeout_ms".into(),
        Value::Number(policy.call_timeout_ms.into()),
    );

    if !policy.allowed_tools.is_empty() {
        map.insert(
            "allowed_tools".into(),
            Value::Array(
                policy
                    .allowed_tools
                    .iter()
                    .map(|tool| Value::String(tool.clone()))
                    .collect(),
            ),
        );
    }
    if !policy.allowed_domains.is_empty() {
        map.insert(
            "allowed_domains".into(),
            Value::Array(
                policy
                    .allowed_domains
                    .iter()
                    .map(|domain| Value::String(domain.clone()))
                    .collect(),
            ),
        );
    }
    Value::Object(map)
}

fn timeouts_to_value(timeouts: &TransportTimeouts) -> Value {
    json!({
        "connect_timeout_ms": timeouts.connect_timeout_ms,
        "read_timeout_ms": timeouts.read_timeout_ms,
        "write_timeout_ms": timeouts.write_timeout_ms,
    })
}

fn heartbeat_to_value(heartbeat: &HeartbeatConfig) -> Value {
    json!({
        "interval_ms": heartbeat.interval_ms,
        "tolerance": heartbeat.tolerance,
    })
}

fn is_default_policy(policy: &PolicyConfig) -> bool {
    policy == &app_default_policy()
}

fn apply_policy_defaults(mut config: McpConfig) -> McpConfig {
    let lib_defaults = PolicyConfig::default();
    if config.policy == lib_defaults {
        config.policy = app_default_policy();
    }
    config
}

fn app_default_policy() -> PolicyConfig {
    PolicyConfig {
        handshake_timeout_ms: defaults::MCP_POLICY_HANDSHAKE_TIMEOUT_MS,
        call_timeout_ms: defaults::MCP_POLICY_CALL_TIMEOUT_MS,
        ..PolicyConfig::default()
    }
}

fn is_default_timeouts(timeouts: &TransportTimeouts) -> bool {
    timeouts == &TransportTimeouts::default()
}
