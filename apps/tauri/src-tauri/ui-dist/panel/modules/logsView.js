import { logState } from "./state.js";

function renderLines(output) {
  if (!output) {
    return;
  }

  if (!logState.lines.length) {
    output.textContent = "// No log entries yet";
    return;
  }

  output.textContent = logState.lines.join("\n");
  output.scrollTop = output.scrollHeight;
}

function updateStatus(statusElement) {
  if (!statusElement) {
    return;
  }

  if (logState.loading) {
    statusElement.textContent = "Loading logs…";
    return;
  }

  if (logState.error) {
    statusElement.textContent = `Error: ${logState.error}`;
    return;
  }

  if (logState.source) {
    const time = logState.lastFetchedAt
      ? new Date(logState.lastFetchedAt).toLocaleTimeString()
      : "";
    statusElement.textContent = `${logState.source}${time ? ` · ${time}` : ""}`;
    return;
  }

  statusElement.textContent = "No log files found";
}

export function createLogsView({ output, status }) {
  const setLoading = (value) => {
    logState.loading = Boolean(value);
    updateStatus(status);
  };

  const setError = (message) => {
    logState.error = message ? String(message) : null;
    updateStatus(status);
  };

  const setLogs = ({ path, lines }) => {
    logState.source = path || null;
    logState.lines = Array.isArray(lines) ? lines : [];
    logState.lastFetchedAt = Date.now();
    logState.error = null;
    renderLines(output);
    updateStatus(status);
  };

  const clear = () => {
    logState.lines = [];
    logState.source = null;
    logState.error = null;
    renderLines(output);
    updateStatus(status);
  };

  renderLines(output);
  updateStatus(status);

  return {
    setLoading,
    setError,
    setLogs,
    clear,
  };
}
