const STATUS = {
  unknown: "unknown",
  starting: "starting",
  running: "running",
  stopped: "stopped",
  error: "error",
  disabled: "disabled",
};

const STATUS_LABELS = {
  [STATUS.unknown]: "Unknown",
  [STATUS.starting]: "Starting",
  [STATUS.running]: "Running",
  [STATUS.stopped]: "Stopped",
  [STATUS.error]: "Error",
  [STATUS.disabled]: "Disabled",
};

const STATUS_CLASSES = {
  [STATUS.unknown]: "is-unknown",
  [STATUS.starting]: "is-starting",
  [STATUS.running]: "is-running",
  [STATUS.stopped]: "is-stopped",
  [STATUS.error]: "is-error",
  [STATUS.disabled]: "is-disabled",
};

function defaultServer(id) {
  return {
    id,
    displayName: null,
    serverName: null,
    serverVersion: null,
    transport: null,
    enabled: true,
    status: STATUS.unknown,
    statusNote: null,
    statusChangedAt: null,
    toolCount: 0,
    resourceCount: 0,
    updatedAtMs: null,
    tools: [],
  };
}

function formatUpdatedAt(timestamp) {
  if (!timestamp) {
    return "No schema sync yet.";
  }
  const date = new Date(Number(timestamp));
  if (Number.isNaN(date.getTime())) {
    return "Last sync unknown.";
  }
  return `Last sync ${date.toLocaleString()}`;

}

function clearStatusClasses(element) {
  if (!element) {
    return;
  }
  element.classList.remove("status-message--error", "status-message--success", "status-message--info");
}

export function createMcpRegistryView({ root, list, placeholder, status, invoke }) {
  if (!root || !list) {
    return null;
  }

  const state = {
    servers: new Map(),
    order: [],
    loading: false,
    loaded: false,
    pendingToggles: new Set(),
  };

  function setStatusMessage(message = "", variant = "info") {
    if (!status) {
      return;
    }
    status.textContent = message || "";
    clearStatusClasses(status);
    if (message) {
      status.classList.add(`status-message--${variant}`);
    }
  }

  function ensureServer(id) {
    if (!state.servers.has(id)) {
      state.servers.set(id, defaultServer(id));
      state.order.push(id);
    }
    return state.servers.get(id);
  }

  function orderedServers() {
    const seen = new Set();
    const ordered = [];
    state.order.forEach((id) => {
      const entry = state.servers.get(id);
      if (entry) {
        ordered.push(entry);
        seen.add(id);
      }
    });
    state.servers.forEach((entry, id) => {
      if (!seen.has(id)) {
        ordered.push(entry);
      }
    });
    return ordered;
  }

  function render() {
    const servers = orderedServers();
    list.innerHTML = "";
    if (!servers.length) {
      if (placeholder) {
        placeholder.hidden = false;
      }
      return;
    }
    if (placeholder) {
      placeholder.hidden = true;
    }
    servers.forEach((server) => {
      list.appendChild(createServerElement(server));
    });
  }

  function createServerElement(server) {
    const card = document.createElement("article");
    card.classList.add("panel-card", "mcp-registry__server");

    const header = document.createElement("header");
    header.classList.add("mcp-registry__header");

    const titles = document.createElement("div");
    titles.classList.add("mcp-registry__titles");

    const title = document.createElement("h3");
    title.classList.add("mcp-registry__title");
    title.textContent = server.displayName || server.serverName || server.id;
    titles.appendChild(title);

    const subtitle = document.createElement("p");
    subtitle.classList.add("mcp-registry__subtitle");
    const subtitleParts = [];
    if (server.displayName && server.displayName !== server.id) {
      subtitleParts.push(server.id);
    }
    if (server.transport) {
      subtitleParts.push(server.transport.toUpperCase());
    }
    if (server.serverVersion) {
      subtitleParts.push(`v${server.serverVersion}`);
    }
    subtitle.textContent = subtitleParts.join(" • ") || server.id;
    titles.appendChild(subtitle);

    header.appendChild(titles);

    const controls = document.createElement("div");
    controls.classList.add("mcp-registry__controls");

    const statusBadge = document.createElement("span");
    statusBadge.classList.add("mcp-registry__status");
    const statusClass = STATUS_CLASSES[server.status] || STATUS_CLASSES[STATUS.unknown];
    statusBadge.classList.add(statusClass);
    statusBadge.textContent = STATUS_LABELS[server.status] || STATUS_LABELS[STATUS.unknown];
    controls.appendChild(statusBadge);

    const toggleButton = document.createElement("button");
    toggleButton.type = "button";
    toggleButton.classList.add("button", "button--ghost", "button--sm", "mcp-registry__toggle");
    toggleButton.textContent = server.enabled ? "Disable" : "Enable";
    toggleButton.title = server.enabled ? "Disable this server" : "Enable this server";
    const togglePending = state.pendingToggles.has(server.id);
    toggleButton.disabled = togglePending || !invoke;
    toggleButton.addEventListener("click", () => {
      toggleServer(server, !server.enabled);
    });
    controls.appendChild(toggleButton);

    header.appendChild(controls);

    card.appendChild(header);
    card.classList.toggle("is-disabled", !server.enabled);

    const meta = document.createElement("p");
    meta.classList.add("mcp-registry__meta");
    const metaParts = [];
    metaParts.push(`Tools ${server.toolCount}`);
    metaParts.push(`Resources ${server.resourceCount}`);
    meta.textContent = metaParts.join(" • ");
    card.appendChild(meta);

    const toolsBlock = document.createElement("div");
    toolsBlock.classList.add("mcp-registry__tools");

    if (server.tools.length) {
      server.tools.forEach((tool) => {
        const chip = document.createElement("span");
        chip.classList.add("mcp-registry__tool");
        chip.textContent = tool.name;
        if (tool.description && tool.description.trim()) {
          chip.title = tool.description.trim();
        } else {
          chip.removeAttribute("title");
        }
        toolsBlock.appendChild(chip);
      });
    } else {
      const empty = document.createElement("p");
      empty.classList.add("mcp-registry__tools-empty");
      empty.textContent = state.loaded
        ? "No tools reported yet."
        : "Waiting for schemas...";
      toolsBlock.appendChild(empty);
    }

    card.appendChild(toolsBlock);

    if (server.statusNote) {
      const note = document.createElement("p");
      note.classList.add("mcp-registry__note");
      note.textContent = server.statusNote;
      card.appendChild(note);
    }

    const updated = document.createElement("p");
    updated.classList.add("mcp-registry__updated");
    updated.textContent = formatUpdatedAt(server.updatedAtMs);
    card.appendChild(updated);

    return card;
  }

  function applyConfigSummary(mcpSummary) {
    const declaredServers = Array.isArray(mcpSummary?.servers) ? mcpSummary.servers : [];
    if (declaredServers.length) {
      const orderedIds = [];
      declaredServers.forEach((server) => {
        if (!server?.id) {
          return;
        }
        const entry = ensureServer(server.id);
        entry.displayName = server.display_name || null;
        entry.transport = server.transport || entry.transport;
        entry.enabled = server.enabled !== false;
        if (!entry.enabled) {
          entry.status = STATUS.disabled;
          entry.statusNote = "Disabled in configuration";
        } else if (entry.status === STATUS.disabled) {
          entry.status = STATUS.unknown;
          entry.statusNote = null;
        }
        orderedIds.push(server.id);
      });
      state.order = [
        ...orderedIds,
        ...state.order.filter((id) => !orderedIds.includes(id)),
      ];
    }
    render();
  }

  function applyRegistrySnapshot(registryEntries = []) {
    if (!Array.isArray(registryEntries)) {
      return;
    }
    registryEntries.forEach((entry) => {
      if (!entry || !entry.id) {
        return;
      }
      const server = ensureServer(entry.id);
      server.displayName = entry.display_name || server.displayName;
      server.serverName = entry.server_name || server.serverName;
      server.serverVersion = entry.server_version || null;
      server.transport = entry.transport || server.transport;
      server.enabled = entry.enabled !== false;
      server.toolCount = typeof entry.tool_count === "number" ? entry.tool_count : entry.tools?.length || 0;
      server.resourceCount =
        typeof entry.resource_count === "number" ? entry.resource_count : server.resourceCount;
      const tools = Array.isArray(entry.tools) ? entry.tools : [];
      server.tools = tools
        .map((tool) => ({
          name: tool?.name || "tool",
          description: typeof tool?.description === "string" ? tool.description : null,
        }))
        .sort((a, b) => a.name.localeCompare(b.name));
      server.toolCount = server.tools.length;
      const updated = Number(entry.updated_at_ms);
      if (!Number.isNaN(updated)) {
        server.updatedAtMs = updated;
      }
      const previousStatus = server.status;
      if (!server.enabled) {
        server.status = STATUS.disabled;
        server.statusNote = "Disabled in configuration";
      } else {
        server.status = STATUS.running;
        if (previousStatus === STATUS.error) {
          server.statusNote = null;
        }
      }
    });
    state.loaded = true;
    render();
  }

  function applySchemaSnapshot(server, payload, timestamp) {
    const tools = Array.isArray(payload?.tool_schemas) ? payload.tool_schemas : [];
    server.tools = tools
      .map((tool) => ({
        name: typeof tool?.name === "string" ? tool.name : "tool",
        description:
          typeof tool?.description === "string" && tool.description.trim()
            ? tool.description
            : null,
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
    server.toolCount = server.tools.length;
    if (Array.isArray(payload?.resources)) {
      server.resourceCount = payload.resources.length;
    }
    server.updatedAtMs = timestamp || Date.now();
    state.loaded = true;
  }

  function updateStatus(server, statusValue, note, timestamp) {
    server.status = statusValue;
    server.statusNote = note || null;
    server.statusChangedAt = timestamp || Date.now();
    if (statusValue === STATUS.running && !note) {
      server.statusNote = null;
    }
  }

  async function toggleServer(server, nextEnabled) {
    if (!invoke || !server?.id) {
      return;
    }
    if (state.pendingToggles.has(server.id)) {
      return;
    }
    state.pendingToggles.add(server.id);
    render();
    const displayName = server.displayName || server.serverName || server.id;
    setStatusMessage(
      `${nextEnabled ? "Enabling" : "Disabling"} ${displayName}...`,
      "info"
    );
    try {
      const summary = await invoke("toggle_mcp_server", {
        serverId: server.id,
        enabled: nextEnabled,
      });
      if (summary && typeof summary === "object") {
        applyConfigSummary(summary);
        setStatusMessage(
          `${displayName} ${nextEnabled ? "enabled" : "disabled"}.`,
          "success"
        );
      }
    } catch (error) {
      console.warn("Failed to toggle MCP server", error);
      setStatusMessage(
        `Failed to ${nextEnabled ? "enable" : "disable"} ${displayName}.`,
        "error"
      );
    } finally {
      state.pendingToggles.delete(server.id);
      render();
    }
  }

  function handleEvent(payload) {
    if (!payload || typeof payload !== "object") {
      return;
    }
    const serverId = payload.meta?.server_id;
    if (!serverId) {
      return;
    }
    const server = ensureServer(serverId);
    const timestamp = Number(payload.meta?.timestamp_ms) || Date.now();
    const eventName = payload.kind?.event;
    switch (eventName) {
      case "server_started": {
        const pid = payload.kind?.pid;
        const note = pid ? `PID ${pid}` : null;
        updateStatus(server, STATUS.running, note, timestamp);
        break;
      }
      case "server_stopped": {
        const exitCode =
          typeof payload.kind?.exit_code === "number" ? payload.kind.exit_code : null;
        const note = exitCode !== null ? `Exited with code ${exitCode}` : "Server stopped";
        updateStatus(server, STATUS.stopped, note, timestamp);
        break;
      }
      case "server_crashed": {
        const message =
          typeof payload.kind?.message === "string" && payload.kind.message.trim()
            ? payload.kind.message.trim()
            : "Server crashed";
        updateStatus(server, STATUS.error, message, timestamp);
        break;
      }
      case "schemas_updated": {
        applySchemaSnapshot(server, payload.kind, timestamp);
        updateStatus(server, STATUS.running, server.statusNote, timestamp);
        break;
      }
      default:
        return;
    }
    render();
  }

  async function loadRegistrySnapshot() {
    if (!invoke || state.loading) {
      return;
    }
    state.loading = true;
    try {
      setStatusMessage("Loading MCP registry...", "info");
      const snapshot = await invoke("get_mcp_registry");
      applyRegistrySnapshot(Array.isArray(snapshot) ? snapshot : []);
      if (state.servers.size) {
        setStatusMessage("", "info");
      } else {
        setStatusMessage("No MCP servers configured.", "info");
      }
    } catch (error) {
      console.warn("Failed to load MCP registry", error);
      setStatusMessage("Failed to load MCP registry.", "error");
    } finally {
      state.loading = false;
    }
  }

  return {
    applyConfigSummary,
    handleEvent,
    refresh: () => loadRegistrySnapshot(),
    onEnter: () => {
      if (!state.loaded) {
        loadRegistrySnapshot();
      }
    },
    setStatus: setStatusMessage,
  };
}
