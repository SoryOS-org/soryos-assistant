import { mcpState } from "./state.js";

const STATUS_LABELS = {
  queued: "Queued",
  running: "Running",
  completed: "Completed",
  crashed: "Crashed",
};

const STATUS_CLASSES = {
  queued: "is-queued",
  running: "is-running",
  completed: "is-completed",
  crashed: "is-crashed",
};

function summarizeEntries(entries = []) {
  return entries.reduce(
    (acc, entry) => {
      const status = String(entry.status || "").toLowerCase();
      acc.total += 1;
      if (status === "running") {
        acc.running += 1;
      } else if (status === "crashed") {
        acc.crashed += 1;
      } else if (status === "completed") {
        acc.completed += 1;
      } else {
        acc.queued += 1;
      }
      return acc;
    },
    { total: 0, running: 0, queued: 0, completed: 0, crashed: 0 },
  );
}

function buildKey(payload) {
  const token = payload.session_token || "session";
  const step = typeof payload.step_index === "number" ? payload.step_index : 0;
  return `${token}-${step}`;
}

function upsertEntry(payload) {
  if (!payload) {
    return;
  }

  const key = buildKey(payload);
  let entry = mcpState.entries.find((item) => item.key === key);

  if (!entry) {
    entry = {
      key,
      sessionToken: payload.session_token || null,
      stepIndex: typeof payload.step_index === "number" ? payload.step_index : 0,
      serverId: payload.server_id || "",
      toolName: payload.tool_name || "",
      createdAt: Date.now(),
    };
    mcpState.entries.push(entry);
  }

  entry.status = String(payload.status || "").toLowerCase();
  const statusNote = typeof payload.status_note === "string" ? payload.status_note : null;
  const legacySpeech = typeof payload.speech_update === "string" ? payload.speech_update : null;
  entry.statusMessage = statusNote || legacySpeech;
  entry.error = payload.error || null;
  entry.latencyMs = typeof payload.latency_ms === "number" ? payload.latency_ms : null;
  entry.updatedAt = Date.now();

  if (payload.server_id) {
    entry.serverId = payload.server_id;
  }
  if (payload.tool_name) {
    entry.toolName = payload.tool_name;
  }

  if (mcpState.entries.length > (mcpState.maxEntries || 50)) {
    mcpState.entries.sort((a, b) => (a.updatedAt || 0) - (b.updatedAt || 0));
    while (mcpState.entries.length > mcpState.maxEntries) {
      mcpState.entries.shift();
    }
  }
}

function formatLatency(latencyMs) {
  if (typeof latencyMs !== "number" || Number.isNaN(latencyMs) || latencyMs < 0) {
    return null;
  }
  if (latencyMs >= 1000) {
    return `${(latencyMs / 1000).toFixed(latencyMs >= 5000 ? 1 : 2)} s`;
  }
  return `${Math.round(latencyMs)} ms`;
}

function createItemElement(entry) {
  const li = document.createElement("li");
  li.classList.add("mcp-progress__item");

  const statusClass = STATUS_CLASSES[entry.status] || "";
  if (statusClass) {
    li.classList.add(statusClass);
  }

  const header = document.createElement("div");
  header.classList.add("mcp-progress__header");
  const title = document.createElement("span");
  title.classList.add("mcp-progress__title");
  const parts = [];
  if (entry.serverId) {
    parts.push(entry.serverId);
  }
  if (entry.toolName) {
    parts.push(entry.toolName);
  }
  title.textContent = parts.length ? parts.join(" · ") : `Step ${entry.stepIndex}`;
  header.appendChild(title);

  const badge = document.createElement("span");
  badge.classList.add("mcp-progress__status");
  badge.textContent = STATUS_LABELS[entry.status] || "Pending";
  header.appendChild(badge);

  li.appendChild(header);

  if (entry.statusMessage && entry.statusMessage.trim()) {
    const speech = document.createElement("p");
    speech.classList.add("mcp-progress__speech");
    speech.textContent = entry.statusMessage.trim();
    li.appendChild(speech);
  }

  if (entry.error && entry.error.trim()) {
    const error = document.createElement("p");
    error.classList.add("mcp-progress__error");
    error.textContent = entry.error.trim();
    li.appendChild(error);
  }

  const meta = document.createElement("div");
  meta.classList.add("mcp-progress__meta");
  const infoParts = [];
  if (entry.sessionToken) {
    infoParts.push(`Session ${entry.sessionToken.slice(0, 8)}`);
  }
  infoParts.push(`Step ${entry.stepIndex}`);

  const latency = formatLatency(entry.latencyMs);
  if (latency) {
    infoParts.push(`Latency ${latency}`);
  }

  meta.textContent = infoParts.join(" • ");
  li.appendChild(meta);

  return li;
}

function render(list, placeholder) {
  if (!list) {
    return;
  }

  list.innerHTML = "";
  const entries = [...mcpState.entries].sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));

  if (!entries.length) {
    if (placeholder) {
      placeholder.hidden = false;
    }
    return;
  }

  if (placeholder) {
    placeholder.hidden = true;
  }

  entries.forEach((entry) => {
    list.appendChild(createItemElement(entry));
  });
}

export function createMcpProgressView({ list, placeholder }) {
  const handle = (payload) => {
    if (!payload || typeof payload !== "object") {
      return;
    }
    upsertEntry(payload);
    render(list, placeholder);
  };

  const clear = () => {
    mcpState.entries = [];
    render(list, placeholder);
  };

  const getSummary = () => summarizeEntries(mcpState.entries);

  render(list, placeholder);

  return {
    handle,
    clear,
    getSummary,
  };
}
