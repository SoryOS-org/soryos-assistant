use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub type ServerId = String;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HostConfig {
    /// Map of logical server identifiers to their runtime configuration.
    #[serde(default, rename = "servers", alias = "mcpServers")]
    pub servers: BTreeMap<ServerId, ServerConfig>,
    /// Global policy defaults applied when server-level overrides are absent.
    pub policy: PolicyConfig,
}

impl HostConfig {
    /// Creates a new config state backed by an `Arc`.
    pub fn into_state(self) -> ConfigState {
        ConfigState::new(self)
    }

    /// Returns a server configuration if one exists for the provided id.
    pub fn server(&self, id: &str) -> Option<&ServerConfig> {
        self.servers.get(id)
    }

    /// Returns `true` if the config contains no servers.
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty()
    }
}

#[derive(Clone)]
pub struct ConfigState {
    inner: Arc<RwLock<HostConfig>>,
}

impl ConfigState {
    pub fn new(config: HostConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(config)),
        }
    }

    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, HostConfig> {
        self.inner.read().expect("config state poisoned")
    }

    pub fn update(&self, config: HostConfig) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = config;
        } else {
            panic!("config state poisoned");
        }
    }

    pub fn clone_host_config(&self) -> HostConfig {
        self.read().clone()
    }

    pub fn policy_snapshot(&self) -> PolicyConfig {
        self.read().policy.clone()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(default)]
pub struct ServerConfig {
    /// User-facing server display name; defaults to the server id.
    pub display_name: Option<String>,
    /// Whether the server should be launched by the host (defaults to true).
    #[serde(skip_serializing_if = "is_enabled_default")]
    pub enabled: bool,
    /// Transport configuration that describes how to reach the server.
    pub transport: TransportConfig,
    /// Optional command that prepares the environment before the main transport starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare: Option<ServerTemplate>,
    /// Access policy limiting tool usage.
    pub policy: PolicyConfig,
    /// Arbitrary metadata forwarded to downstream consumers (e.g. worker prompts).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            display_name: None,
            enabled: true,
            transport: TransportConfig::default(),
            prepare: None,
            policy: PolicyConfig::default(),
            metadata: BTreeMap::new(),
        }
    }
}

fn is_enabled_default(value: &bool) -> bool {
    *value
}

#[derive(Default, Deserialize)]
struct RawServerConfig {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    transport: Option<TransportConfig>,
    #[serde(default)]
    prepare: Option<ServerTemplate>,
    #[serde(default)]
    policy: PolicyConfig,
    #[serde(default)]
    metadata: BTreeMap<String, Value>,
    #[serde(default, flatten)]
    legacy: LegacyTransportFields,
}

#[derive(Default, Deserialize)]
struct LegacyTransportFields {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(rename = "current_dir")]
    current_dir: Option<String>,
    #[serde(default)]
    timeouts: Option<TransportTimeouts>,
    #[serde(default)]
    heartbeat: Option<HeartbeatConfig>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

impl LegacyTransportFields {
    fn into_transport(self) -> std::result::Result<Option<TransportConfig>, String> {
        let kind = match self.kind {
            Some(kind) => kind,
            None => {
                if self.command.is_some()
                    || !self.args.is_empty()
                    || !self.env.is_empty()
                    || self.current_dir.is_some()
                    || self.timeouts.is_some()
                    || self.heartbeat.is_some()
                {
                    "stdio".to_string()
                } else {
                    return Ok(None);
                }
            }
        };

        match kind.as_str() {
            "stdio" => {
                let command = self
                    .command
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "missing `command` for stdio transport".to_string())?;

                Ok(Some(TransportConfig::Stdio {
                    command,
                    args: self.args,
                    env: self.env,
                    current_dir: self.current_dir,
                    timeouts: self.timeouts.unwrap_or_default(),
                    heartbeat: self.heartbeat,
                }))
            }
            "ws" => {
                let url = self
                    .url
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "missing `url` for ws transport".to_string())?;

                Ok(Some(TransportConfig::Ws {
                    url,
                    headers: self.headers,
                    timeouts: self.timeouts.unwrap_or_default(),
                    heartbeat: self.heartbeat,
                }))
            }
            "http" => {
                let url = self
                    .url
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "missing `url` for http transport".to_string())?;

                Ok(Some(TransportConfig::Http {
                    url,
                    headers: self.headers,
                    timeouts: self.timeouts.unwrap_or_default(),
                    heartbeat: self.heartbeat,
                }))
            }
            other => Err(format!("unsupported transport type `{other}`")),
        }
    }
}

impl<'de> Deserialize<'de> for ServerConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawServerConfig::deserialize(deserializer)?;
        let RawServerConfig {
            display_name,
            enabled,
            transport,
            prepare,
            policy,
            metadata,
            legacy,
        } = raw;

        let transport = if let Some(config) = transport {
            config
        } else {
            legacy
                .into_transport()
                .map_err(SerdeDeError::custom)?
                .unwrap_or_default()
        };

        Ok(Self {
            display_name,
            enabled: enabled.unwrap_or(true),
            transport,
            prepare,
            policy,
            metadata,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PolicyConfig {
    /// Max duration (ms) to wait for handshake completion.
    pub handshake_timeout_ms: u64,
    /// Max duration (ms) to wait for an individual call.
    pub call_timeout_ms: u64,
    /// Allow-listed tool names; empty implies all tools are permitted.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_tools: BTreeSet<String>,
    /// Allow-listed domain scopes.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_domains: BTreeSet<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            handshake_timeout_ms: 10_000,
            // Zero disables the per-call timeout; servers can override per-policy.
            call_timeout_ms: 0,
            allowed_tools: BTreeSet::new(),
            allowed_domains: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportConfig {
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_dir: Option<String>,
        #[serde(default)]
        timeouts: TransportTimeouts,
        #[serde(skip_serializing_if = "Option::is_none")]
        heartbeat: Option<HeartbeatConfig>,
    },
    Ws {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        timeouts: TransportTimeouts,
        #[serde(skip_serializing_if = "Option::is_none")]
        heartbeat: Option<HeartbeatConfig>,
    },
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        timeouts: TransportTimeouts,
        #[serde(skip_serializing_if = "Option::is_none")]
        heartbeat: Option<HeartbeatConfig>,
    },
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::Stdio {
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            current_dir: None,
            timeouts: TransportTimeouts::default(),
            heartbeat: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TransportTimeouts {
    /// Milliseconds allowed for initial connection or child start-up.
    pub connect_timeout_ms: u64,
    /// Milliseconds allowed between reads before considering the transport stalled.
    pub read_timeout_ms: u64,
    /// Milliseconds allowed between writes.
    pub write_timeout_ms: u64,
}

impl Default for TransportTimeouts {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 10_000,
            read_timeout_ms: 60_000,
            write_timeout_ms: 60_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HeartbeatConfig {
    /// Interval (ms) between heartbeat requests.
    pub interval_ms: u64,
    /// Maximum tolerated miss count before considering the server unhealthy.
    pub tolerance: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_ms: 15_000,
            tolerance: 3,
        }
    }
}

/// Shell command template used to prepare the environment before main transport start.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerTemplate {
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_stdio() {
        let mut host = HostConfig::default();
        let server = ServerConfig {
            display_name: Some("Local Browser".into()),
            enabled: true,
            transport: TransportConfig::Stdio {
                command: "mcp-browser.exe".into(),
                args: vec!["--serve".into()],
                env: BTreeMap::from([("RUST_LOG".into(), "info".into())]),
                current_dir: Some("C:/tools/browser".into()),
                timeouts: TransportTimeouts {
                    connect_timeout_ms: 20_000,
                    read_timeout_ms: 45_000,
                    write_timeout_ms: 45_000,
                },
                heartbeat: Some(HeartbeatConfig {
                    interval_ms: 5_000,
                    tolerance: 2,
                }),
            },
            prepare: Some(ServerTemplate {
                command: "ensure-assets.exe".into(),
                args: vec!["--sync".into()],
                env: BTreeMap::new(),
                current_dir: None,
            }),
            policy: PolicyConfig {
                handshake_timeout_ms: 5_000,
                call_timeout_ms: 15_000,
                allowed_tools: BTreeSet::from(["search".into(), "open_url".into()]),
                allowed_domains: BTreeSet::new(),
            },
            metadata: BTreeMap::from([("category".into(), Value::from("browsing"))]),
        };
        host.servers.insert("browser".into(), server);

        let json = serde_json::to_string_pretty(&host).expect("serialize");
        let parsed: HostConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(parsed.server("browser").is_some());
    }

    #[test]
    fn enabled_field_serializes_only_when_false() {
        let json = r#"
        {
            "servers": {
                "alpha": {
                    "command": "bin.exe",
                    "args": ["--serve"]
                },
                "beta": {
                    "enabled": false,
                    "command": "bin.exe",
                    "args": ["--serve"]
                }
            },
            "policy": {}
        }
        "#;

        let config: HostConfig = serde_json::from_str(json).expect("deserialize config");
        assert!(config.server("alpha").unwrap().enabled);
        assert!(!config.server("beta").unwrap().enabled);

        let round = serde_json::to_string(&config).expect("serialize");
        assert!(round.contains("\"enabled\":false"));
        assert!(!round.contains("\"enabled\":true"));
    }

    #[test]
    fn default_policy_values() {
        let config = HostConfig::default();
        assert_eq!(config.policy.handshake_timeout_ms, 10_000);
        assert_eq!(config.policy.call_timeout_ms, 0);
        assert!(config.policy.allowed_domains.is_empty());

        let state = config.into_state();
        {
            let guard = state.read();
            assert_eq!(guard.policy.handshake_timeout_ms, 10_000);
        }

        let mut updated = HostConfig::default();
        updated.policy.call_timeout_ms = 5_000;
        state.update(updated.clone());
        assert_eq!(state.policy_snapshot().call_timeout_ms, 5_000);
        assert_eq!(state.clone_host_config().policy.call_timeout_ms, 5_000);
    }

    #[test]
    fn legacy_fields_default_to_stdio_when_type_missing() {
        let json = r#"
        {
            "servers": {
                "alpha": {
                    "command": "bin.exe",
                    "args": ["--serve"],
                    "env": {
                        "RUST_LOG": "debug"
                    }
                }
            },
            "policy": {}
        }
        "#;

        let config: HostConfig = serde_json::from_str(json).expect("deserialize config");
        let server = config.server("alpha").expect("server present");

        match &server.transport {
            TransportConfig::Stdio {
                command, args, env, ..
            } => {
                assert_eq!(command, "bin.exe");
                assert_eq!(args, &["--serve".to_string()]);
                assert_eq!(env.get("RUST_LOG"), Some(&"debug".to_string()));
            }
            other => panic!("expected stdio transport, got {:?}", other),
        }
    }
}
