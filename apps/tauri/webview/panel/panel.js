import { EVENT_CHANNELS } from "./modules/constants.js";
import { setModelCatalog } from "./modules/catalog.js";
import { createPreloadBanner } from "./modules/preloadBanner.js";
import { createConfigEditor } from "./modules/config/editor.js";
import { createChatView } from "./modules/chatView.js";
import { createLogsView } from "./modules/logsView.js";
import { createMcpProgressView } from "./modules/mcpProgress.js";
import { createMcpRegistryView } from "./modules/mcpRegistry.js";
import { createPreferencesView } from "./modules/preferencesView.js";
import { configState, uiState, logState } from "./modules/state.js";
import { createNavigation } from "./modules/navigation.js";
import { createLatencyHud } from "./modules/latencyHud.js";
import { createSecretsController } from "./modules/secrets.js";
import { createAboutMetadata } from "./modules/about.js";
import { createMcpPanel } from "./modules/mcpPanel.js";
import { createChatControls } from "./modules/chatControls.js";

const tauri = window.__TAURI__ || {};
const tauriCore = tauri.core;
const tauriEvent = tauri.event;
const tauriWindow = tauri.window;
const tauriShell = tauri.shell;
const invoke = tauriCore?.invoke;

const suppressContext = (event) => {
  event.preventDefault();
};

document.addEventListener("contextmenu", suppressContext, { capture: true });
window.addEventListener("contextmenu", suppressContext, { capture: true });

function openExternalUrl(url) {
  if (!url) {
    return Promise.resolve();
  }
  if (tauriShell?.open) {
    return tauriShell.open(url);
  }
  window.open(url, "_blank", "noopener,noreferrer");
  return Promise.resolve();
}

const stateElements = {
  pill: document.querySelector('[data-field="ui-state-pill"]'),
  label: document.querySelector('[data-field="ui-state-label"]'),
  meta: document.querySelector('[data-field="ui-state-updated"]'),
};

const latencyElements = {
  readout: document.querySelector("[data-latency-readout]"),
  meta: document.querySelector("[data-latency-meta]"),
};

const navigation = createNavigation({
  onNavigate: (viewId) => {
    if (!invoke) {
      return;
    }
    invoke("panel_navigate", { view: viewId }).catch((error) => {
      console.error("Failed to request panel navigation", error);
    });
  },
});

const LATENCY_REFRESH_MS = 4000;

const configForm = document.querySelector("#config-edit-form");
const saveButton = document.querySelector("#config-save-btn");
const patchPreview = document.querySelector("#config-patch-preview");
const statusMessage = document.querySelector("#config-status");
const secretStatus = document.querySelector("[data-secret-status]");
const secretPicovoiceRow = document.querySelector("[data-secret='picovoice']");
const secretPicovoicePill = secretPicovoiceRow?.querySelector("[data-secret-pill]");
const secretPicovoiceHint = secretPicovoiceRow?.querySelector("[data-secret-hint]");
const secretPicovoiceHintInput = secretPicovoiceRow?.querySelector("[data-secret-hint-input]");
const secretPicovoiceSetButton = document.querySelector("#secret-picovoice-set");
const secretPicovoiceClearButton = document.querySelector("#secret-picovoice-clear");
const secretWorkerAuthRow = document.querySelector("[data-secret='worker-auth']");
const secretWorkerAuthPill = secretWorkerAuthRow?.querySelector("[data-secret-pill]");
const secretWorkerAuthHint = secretWorkerAuthRow?.querySelector("[data-secret-hint]");
const secretWorkerAuthHintInput = secretWorkerAuthRow?.querySelector("[data-secret-hint-input]");
const secretWorkerAuthSetButton = document.querySelector("#secret-worker-auth-set");
const secretWorkerAuthClearButton = document.querySelector("#secret-worker-auth-clear");
const workerAuthPill = document.querySelector("[data-worker-auth-pill]");
const sttErrorPill = document.querySelector("[data-stt-error-pill]");

function setWorkerAuthVisible(visible) {
  if (!workerAuthPill) {
    return;
  }
  workerAuthPill.hidden = !visible;
}

function setSttErrorVisible(visible) {
  if (!sttErrorPill) {
    return;
  }
  sttErrorPill.hidden = !visible;
}

const secretsController = createSecretsController({
  entries: [
    {
      key: "picovoice",
      row: secretPicovoiceRow,
      pill: secretPicovoicePill,
      hint: secretPicovoiceHint,
      hintInput: secretPicovoiceHintInput,
      setButton: secretPicovoiceSetButton,
      clearButton: secretPicovoiceClearButton,
      setCommand: "set_picovoice_key",
      clearCommand: "clear_picovoice_key",
      prompt: "Enter Picovoice access key (stored securely):",
      savedLabel: "Picovoice key saved",
      clearedLabel: "Picovoice key cleared",
      clearConfirm: "Remove the stored Picovoice key?",
    },
    {
      key: "worker_auth",
      row: secretWorkerAuthRow,
      pill: secretWorkerAuthPill,
      hint: secretWorkerAuthHint,
      hintInput: secretWorkerAuthHintInput,
      setButton: secretWorkerAuthSetButton,
      clearButton: secretWorkerAuthClearButton,
      setCommand: "set_worker_auth_token",
      clearCommand: "clear_worker_auth_token",
      prompt: "Enter worker auth token (stored securely):",
      savedLabel: "Worker auth token saved",
      clearedLabel: "Worker auth token cleared",
      clearConfirm: "Remove the stored worker auth token?",
    },
  ],
  statusLabel: secretStatus,
  invoke,
  onStatusChange: (status) => {
    if (status?.worker_auth?.has_key) {
      setWorkerAuthVisible(false);
    }
  },
});
secretsController.bindActions();

const configDirtyPill = document.querySelector("[data-config-dirty-pill]");

const chatList = document.querySelector("[data-chat-log]");
const chatPlaceholder = document.querySelector("[data-chat-empty]");
const chatResetButton = document.querySelector("#chat-reset-btn");
const chatLayout = document.querySelector("[data-chat-layout]");
const mcpRail = document.querySelector("[data-mcp-rail]");
const mcpSummaryChip = document.querySelector("[data-mcp-summary]");
const mcpToggleButton = document.querySelector("#mcp-toggle-btn");
const chatContextToggleButton = document.querySelector("#chat-context-toggle-btn");
const chatMetaToggleButton = document.querySelector("#chat-meta-toggle-btn");
const chatController = createChatView({
  list: chatList,
  placeholder: chatPlaceholder,
});
const chatControls = createChatControls({
  controller: chatController,
  metaButton: chatMetaToggleButton,
  contextButton: chatContextToggleButton,
});
chatControls.bind();

async function hydrateChatHistory() {
  if (!invoke || !chatController) {
    return;
  }
  try {
    const memory = await invoke("get_conversation_memory");
    chatController.hydrateFromMemory(memory);
  } catch (error) {
    console.warn("Failed to hydrate chat history", error);
  }
}

if (chatResetButton && invoke) {
  chatResetButton.addEventListener("click", () => {
    if (chatResetButton.disabled) {
      return;
    }
    const originalLabel = chatResetButton.textContent;
    chatResetButton.disabled = true;
    chatResetButton.textContent = "Resetting...";

    invoke("reset_worker_session")
      .then(() => {
        chatController.clear();
        chatResetButton.textContent = "Session reset";
        window.setTimeout(() => {
          chatResetButton.textContent = originalLabel;
          chatResetButton.disabled = false;
        }, 1200);
      })
      .catch((error) => {
        console.error("Failed to reset worker session", error);
        chatResetButton.textContent = "Reset failed";
        window.setTimeout(() => {
          chatResetButton.textContent = originalLabel;
          chatResetButton.disabled = false;
        }, 2000);
      });
  });
}

const mcpList = document.querySelector("[data-mcp-progress-list]");
const mcpPlaceholder = document.querySelector("[data-mcp-progress-empty]");
const mcpController = createMcpProgressView({
  list: mcpList,
  placeholder: mcpPlaceholder,
});
const mcpPanel = createMcpPanel({
  layoutEl: chatLayout,
  railEl: mcpRail,
  toggleButton: mcpToggleButton,
  summaryChip: mcpSummaryChip,
  progressController: mcpController,
});

const mcpRegistryController = createMcpRegistryView({
  root: document.querySelector('[data-view="mcp"]'),
  list: document.querySelector("[data-mcp-registry-list]"),
  placeholder: document.querySelector("[data-mcp-registry-empty]"),
  status: document.querySelector("[data-mcp-registry-status]"),
  invoke,
});
if (mcpRegistryController) {
  mcpRegistryController.refresh();
}

const mcpConfigButton = document.querySelector("[data-mcp-open-config]");
if (mcpConfigButton && invoke) {
  mcpConfigButton.addEventListener("click", () => {
    mcpConfigButton.disabled = true;
    if (mcpRegistryController?.setStatus) {
      mcpRegistryController.setStatus("Opening MCP configuration...", "info");
    }
    invoke("open_mcp_config_file")
      .then(() => {
        if (mcpRegistryController?.setStatus) {
          mcpRegistryController.setStatus("Opened MCP configuration.", "success");
        }
      })
      .catch((error) => {
        console.warn("Failed to open MCP config file", error);
        if (mcpRegistryController?.setStatus) {
          mcpRegistryController.setStatus("Failed to open MCP configuration.", "error");
        }
      })
      .finally(() => {
        mcpConfigButton.disabled = false;
      });
  });
}

const logsOutput = document.querySelector("[data-logs-output]");
const logsStatus = document.querySelector("[data-logs-status]");
const logsRefreshButton = document.querySelector("#logs-refresh-btn");
const logsController = createLogsView({
  output: logsOutput,
  status: logsStatus,
});

const preferencesController = createPreferencesView({
  root: document.querySelector('[data-view="preferences"]'),
  invoke,
});

const aboutElements = {
  build: document.querySelector("[data-about-build]"),
  runtime: document.querySelector("[data-about-runtime]"),
  os: document.querySelector("[data-about-os]"),
};
const aboutMetadata = createAboutMetadata({
  buildEl: aboutElements.build,
  runtimeEl: aboutElements.runtime,
  osEl: aboutElements.os,
  invoke,
});

const preloadController = createPreloadBanner({
  banner: document.querySelector("#preload-banner"),
  message: document.querySelector("[data-preload-banner-message]"),
  progressContainer: document.querySelector("[data-preload-banner-progress]"),
  progressBar: document.querySelector("[data-preload-banner-progress-bar]"),
});

const configEditor = createConfigEditor({
  form: configForm,
  saveButton,
  statusElement: statusMessage,
  patchPreview,
});

const {
  setStatus,
  clearStatus,
  populateFormFromSummary,
  collectFormValues,
  buildPatchFromValues,
  updatePatchPreview,
  hasChanges,
} = configEditor;

function setUiStatePill(stateLabel) {
  uiState.state = stateLabel;
  uiState.timestamp = Date.now();

  if (stateElements.label) {
    stateElements.label.textContent = stateLabel;
  }
  if (stateElements.pill) {
    stateElements.pill.dataset.state = stateLabel;
  }
  if (stateElements.meta) {
    const now = new Date();
    stateElements.meta.textContent = `Updated ${now.toLocaleTimeString()}`;
  }
}

function setConfigDirtyVisible(visible) {
  if (!configDirtyPill) {
    return;
  }
  configDirtyPill.hidden = !visible;
}

const latencyHud = createLatencyHud({
  readoutEl: latencyElements.readout,
  metaEl: latencyElements.meta,
  invoke,
  refreshMs: LATENCY_REFRESH_MS,
});

function handleUiEvent(payload = {}) {
  if (payload.kind === "State" && payload.payload) {
    const state = payload.payload.state || "Idle";
    setUiStatePill(state);
  }
}

async function hydrateUiSnapshot() {
  if (!invoke) {
    return;
  }
  try {
    const snapshot = await invoke("get_ui_snapshot");
    if (snapshot?.state) {
      setUiStatePill(snapshot.state.state || "Idle");
    }
  } catch (error) {
    console.warn("Failed to hydrate UI snapshot", error);
  }
}

function applySummary(summary) {
  if (!summary) {
    return;
  }
  configState.summary = summary;
  configState.loaded = true;
  setModelCatalog(summary.models || []);
  populateFormFromSummary(summary);
  if (mcpRegistryController) {
    mcpRegistryController.applyConfigSummary(summary.mcp);
  }
}

async function loadConfigSummary(showLoading = false) {
  if (!invoke || configState.loading) {
    return;
  }

  configState.loading = true;
  if (showLoading) {
    setStatus("Loading configuration?", "info");
  }

  try {
    const summary = await invoke("get_config_summary");
    applySummary(summary);
    updatePatchPreview();
    secretsController.loadStatus();
    clearStatus();
  } catch (error) {
    console.error("Failed to load configuration summary", error);
    setStatus("Failed to load configuration.", "error");
  } finally {
    configState.loading = false;
  }
}

async function refreshLogs() {
  if (!invoke || logState.loading) {
    return;
  }

  logsController.setLoading(true);

  try {
    const snapshot = await invoke("get_recent_logs", { limit: 200 });
    if (snapshot && typeof snapshot === "object") {
      logsController.setLogs({
        path: snapshot.path || "",
        lines: Array.isArray(snapshot.lines) ? snapshot.lines : [],
      });
    } else {
      logsController.setLogs({ path: "", lines: [] });
    }
  } catch (error) {
    console.error("Failed to load recent logs", error);
    const reason = typeof error === "string" ? error : error?.message;
    logsController.setError(reason || "Failed to load logs");
  } finally {
    logsController.setLoading(false);
  }
}

async function saveConfigChanges() {
  if (!invoke || !configForm) {
    return;
  }

  if (!configForm.reportValidity()) {
    return;
  }

  const values = collectFormValues();
  const patch = buildPatchFromValues(values);

  if (!hasChanges(patch)) {
    setStatus("No changes to save.", "info");
    return;
  }

    setStatus("Saving changes...", "info");
  saveButton.disabled = true;

  try {
    const response = await invoke("update_config", { patch });
    if (response?.summary) {
      applySummary(response.summary);
    }
    setStatus("Configuration updated successfully.", "success");
    setConfigDirtyVisible(false);
    updatePatchPreview();
  } catch (error) {
    console.error("Failed to update configuration", error);
    const reason = typeof error === "string" ? error : error?.message;
    setStatus(`Failed to update configuration: ${reason || "Unknown error"}`, "error");
  } finally {
    saveButton.disabled = false;
  }
}

const hideButton = document.querySelector("#panel-hide-btn");
if (hideButton) {
  hideButton.addEventListener("click", () => {
    if (!invoke) {
      tauriWindow?.getCurrent?.()?.hide?.();
      return;
    }

    invoke("panel_hide").catch((error) => {
      console.warn("Failed to hide panel via command", error);
      tauriWindow?.getCurrent?.()?.hide?.();
    });
  });
}

if (configForm) {
  configForm.addEventListener("submit", (event) => {
    event.preventDefault();
    saveConfigChanges();
  });

  configForm.addEventListener("input", () => {
    updatePatchPreview();
  });
}

if (saveButton) {
  saveButton.addEventListener("click", () => {
    saveConfigChanges();
  });
}

document.querySelectorAll(".panel-nav__item").forEach((button) => {
  const viewId = button.dataset.view;
  if (!viewId) {
    return;
  }
  button.addEventListener("click", () => navigation.requestNavigation(viewId));
});

if (logsRefreshButton) {
  logsRefreshButton.addEventListener("click", () => {
    refreshLogs();
  });
}

document.querySelectorAll("[data-external-link], .panel-about__links a").forEach((link) => {
  link.addEventListener("click", (event) => {
    event.preventDefault();
    const href = link.getAttribute("href");
    openExternalUrl(href).catch((error) => {
      console.warn("Failed to open external link", error);
    });
  });
});

mcpPanel.init();

navigation.registerOnEnter("config", () => loadConfigSummary(false));
if (preferencesController) {
  navigation.registerOnEnter("preferences", () => preferencesController.onEnter());
}
if (mcpRegistryController) {
  navigation.registerOnEnter("mcp", () => mcpRegistryController.onEnter());
}
navigation.registerOnEnter("chat", () => chatController.onEnter());
navigation.registerOnEnter("logs", () => refreshLogs());

if (tauriEvent?.listen) {
  tauriEvent
    .listen(EVENT_CHANNELS.navigation, (event) => {
      if (typeof event.payload === "string") {
        navigation.activate(event.payload);
      }
    })
    .catch((error) => {
      console.warn("Failed to subscribe to panel navigation channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.ui, (event) => {
      handleUiEvent(event.payload);
    })
    .catch((error) => {
      console.warn("Failed to subscribe to UI channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.configChanged, () => {
      configState.loaded = false;
      setConfigDirtyVisible(false);
      loadConfigSummary(true)
        .then(() => setStatus("Configuration reloaded from disk.", "info"))
        .catch((error) => {
          console.warn("Failed to reload config summary after change event", error);
          setStatus("Failed to reload configuration after change.", "error");
        });
    })
    .catch((error) => {
      console.warn("Failed to subscribe to config change channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.configDirty, (event) => {
      setConfigDirtyVisible(true);
      console.warn("STT config changed on disk", event.payload || {});
    })
    .catch((error) => {
      console.warn("Failed to subscribe to config dirty channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.workerAuth, (event) => {
      setWorkerAuthVisible(true);
      console.warn("Worker auth failed", event.payload || {});
    })
    .catch((error) => {
      console.warn("Failed to subscribe to worker auth channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.workerAuthOk, () => {
      setWorkerAuthVisible(false);
    })
    .catch((error) => {
      console.warn("Failed to subscribe to worker auth ok channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.sttError, (event) => {
      setSttErrorVisible(true);
      console.warn("STT failed to start", event.payload || {});
    })
    .catch((error) => {
      console.warn("Failed to subscribe to STT error channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.sttReady, () => {
      setSttErrorVisible(false);
    })
    .catch((error) => {
      console.warn("Failed to subscribe to STT ready channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.preload, (event) => {
      preloadController.handle(event.payload || {});
    })
    .catch((error) => {
      console.warn("Failed to subscribe to preload channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.preferencesChanged, (event) => {
      if (preferencesController && event?.payload) {
        preferencesController.applyExternalSnapshot(event.payload);
      }
    })
    .catch((error) => {
      console.warn("Failed to subscribe to preferences channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.catalogChanged, (event) => {
      if (preferencesController && event?.payload) {
        preferencesController.applyExternalCatalog(event.payload);
      }
    })
    .catch((error) => {
      console.warn("Failed to subscribe to catalog channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.sttFinal, (event) => {
      chatController.addTranscript(event.payload || {});
      latencyHud.refresh();
    })
    .catch((error) => {
      console.warn("Failed to subscribe to STT transcript channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.sttResult, (event) => {
      chatController.addAssistant(event.payload || {});
      latencyHud.refresh();
    })
    .catch((error) => {
      console.warn("Failed to subscribe to STT result channel", error);
    });

  tauriEvent
    .listen(EVENT_CHANNELS.mcpProgress, (event) => {
      mcpController.handle(event.payload || {});
      mcpPanel.applySummary();
    })
    .catch((error) => {
      console.warn("Failed to subscribe to MCP progress channel", error);
    });

  if (mcpRegistryController) {
    tauriEvent
      .listen(EVENT_CHANNELS.mcpStatus, (event) => {
        mcpRegistryController.handleEvent(event.payload || {});
      })
      .catch((error) => {
        console.warn("Failed to subscribe to MCP status channel", error);
      });
  }
}

aboutMetadata.load();
hydrateUiSnapshot();
loadConfigSummary(true);
if (preferencesController) {
  preferencesController.load();
}
hydrateChatHistory();
chatControls.refresh();
latencyHud.start();
navigation.activate(navigation.current(), { force: true });
secretsController.loadStatus();
