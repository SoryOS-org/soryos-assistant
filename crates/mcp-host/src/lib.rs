pub mod client;
pub mod config;
pub mod events;
pub mod host;
pub mod registry;
pub mod router;
pub mod sinks;
pub mod transport;

pub use client::{
    ClientError, ClientInfo, InitializeResponse, McpClient, ResourceDescriptor, ServerInfo,
    ToolSchema,
};
pub use config::{
    ConfigState, HostConfig, PolicyConfig, ServerConfig, ServerId, ServerTemplate, TransportConfig,
};
pub use events::{
    EventMeta, HostEvent, HostEventKind, SchemaSnapshotEvent, ServerCrashedEvent,
    ServerStartedEvent, ServerStoppedEvent,
};
pub use host::{HostError, HostHandle};
pub use registry::{Registry, RegistryEntry};
pub use router::{Router, RouterError};
pub use sinks::HostSinks;
pub use transport::{build_transport, StdioTransport, Transport, TransportError};
