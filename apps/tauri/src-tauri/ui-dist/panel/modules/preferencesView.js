import { preferencesState } from "./state.js";
import { friendlyLabel } from "./catalog.js";

const UPDATE_DEBOUNCE_MS = 150;
const debug = (...args) => console.debug("[PreferencesView]", ...args);
function rawAppId(app = {}) {
  return app.app_id || app.appId || app.identifier || "";
}

function normalizeId(app) {
  return rawAppId(app) || app.name || "";
}

function resolveElement(root, selector) {
  return root?.querySelector(selector) || document.querySelector(selector);
}

function cloneMap(map) {
  return new Map(map);
}

function mapsEqual(a, b) {
  if (a.size !== b.size) {
    return false;
  }
  for (const [key, value] of a) {
    if (!b.has(key) || b.get(key) !== value) {
      return false;
    }
  }
  return true;
}

function ensureTableBody(root, current) {
  if (current && current.isConnected) {
    return current;
  }
  const next = resolveElement(root, "[data-preferences-allowlist-body]");
  if (!next) {
    debug("tableBody:missing", { hasRoot: !!root });
  }
  return next;
}

export function createPreferencesView({ root, invoke }) {
  if (!root || typeof invoke !== "function") {
    return null;
  }

  const statusElement = resolveElement(root, "[data-preferences-status]");
  const form = resolveElement(root, "#preferences-form") || resolveElement(root, "[data-preferences-form]");
  let personalityInput = resolveElement(root, "#preferences-personality");
  const searchInput = resolveElement(root, "#preferences-search");
  let tableBody = resolveElement(root, "[data-preferences-allowlist-body]");
  const saveButton = resolveElement(root, "#preferences-save-btn");
  const cancelButton = resolveElement(root, "#preferences-cancel-btn");
  const restoreButton = resolveElement(root, "#preferences-restore-btn");
  const uncheckAllButton = resolveElement(root, "#preferences-uncheck-all-btn");

  debug("init:elements", {
    hasStatus: !!statusElement,
    hasForm: !!form,
    hasPersonalityInput: !!personalityInput,
    hasSearchInput: !!searchInput,
    hasTableBody: !!tableBody,
    hasSaveButton: !!saveButton,
    hasCancelButton: !!cancelButton,
    hasRestoreButton: !!restoreButton,
    hasUncheckAllButton: !!uncheckAllButton,
  });

  let catalog = [];
  let overrides = new Map();
  let initialOverrides = new Map();
  let defaultPersonality = "Snappy and cheeky";
  let initialPersonality = "";
  let dirty = false;
  let saving = false;
  let filterText = "";
  let debounceId = null;

  function setStatus(message, tone = "info") {
    if (!statusElement) {
      return;
    }
    statusElement.textContent = message;
    statusElement.dataset.state = tone;
  }

  function setControlsDisabled(disabled) {
    const elements = form ? Array.from(form.querySelectorAll("input, textarea, button")) : [];
    elements.forEach((element) => {
      if (element === searchInput) {
        element.disabled = disabled;
      } else if (element === saveButton) {
        element.disabled = disabled || !dirty || saving;
      } else if (element === cancelButton) {
        element.disabled = disabled || !dirty || saving;
      } else {
        element.disabled = disabled;
      }
    });
    if (searchInput) {
      searchInput.disabled = disabled;
    }
    if (saveButton) {
      saveButton.disabled = disabled || !dirty || saving;
    }
    if (cancelButton) {
      cancelButton.disabled = disabled || !dirty || saving;
    }
    if (restoreButton) {
      restoreButton.disabled = disabled;
    }
    if (uncheckAllButton) {
      uncheckAllButton.disabled = disabled;
    }
  }

  function computeSummary() {
    const total = catalog.length;
    const disabledCount = catalog.reduce(
      (count, entry) => (overrides.get(entry.key) === false ? count + 1 : count),
      0
    );
    const allowed = total - disabledCount;
    return { total, allowed, disabled: disabledCount };
  }

  function updateSummaryLabel() {
    const { total, allowed, disabled: disabledCount } = computeSummary();
    const base = total > 0 ? `${allowed}/${total} apps allowed` : "No apps discovered";
    if (dirty) {
      setStatus(`${base} - Unsaved changes`, "pending");
    } else if (disabledCount > 0) {
      setStatus(`${base} (${disabledCount} disabled)`, "info");
    } else {
      setStatus(base, "info");
    }
  }

  function computeDirty() {
    const personalityChanged =
      personalityInput && personalityInput.value.trim() !== initialPersonality;
    const overridesChanged = !mapsEqual(overrides, initialOverrides);
    dirty = personalityChanged || overridesChanged;
    if (saveButton) {
      saveButton.disabled = !dirty || saving;
    }
    if (cancelButton) {
      cancelButton.disabled = !dirty || saving;
    }
    updateSummaryLabel();
  }

  function scheduleDirtyCompute() {
    if (debounceId) {
      window.clearTimeout(debounceId);
    }
    debounceId = window.setTimeout(() => {
      debounceId = null;
      computeDirty();
    }, UPDATE_DEBOUNCE_MS);
  }

  function renderTable() {
    tableBody = ensureTableBody(root, tableBody);
    if (!tableBody) {
      return;
    }
    tableBody.innerHTML = "";
    const query = filterText.trim().toLowerCase();

    const filtered = catalog
      .filter((entry) => {
        if (!query) {
          return true;
        }
        return (
          entry.name.toLowerCase().includes(query) ||
          entry.identifier.toLowerCase().includes(query)
        );
      })
      .sort((a, b) => {
        const aAllowed = overrides.get(a.key) !== false;
        const bAllowed = overrides.get(b.key) !== false;
        if (aAllowed !== bAllowed) {
          return aAllowed ? -1 : 1;
        }
        return a.name.localeCompare(b.name);
      });

    debug("renderTable", { catalogSize: catalog.length, filteredSize: filtered.length, query });

    if (!filtered.length) {
      const row = document.createElement("tr");
      row.classList.add("preferences-empty");
      const cell = document.createElement("td");
      cell.colSpan = 3;
      cell.textContent = query ? "No apps match the current search." : "No installed apps discovered.";
      row.appendChild(cell);
      tableBody.appendChild(row);
      return;
    }

    filtered.forEach((entry) => {
      const row = document.createElement("tr");
      row.dataset.appId = entry.key;

      const nameCell = document.createElement("td");
      nameCell.textContent = entry.name;
      nameCell.title = entry.name;
      row.appendChild(nameCell);

      const idCell = document.createElement("td");
      idCell.classList.add("preferences-col-identifier");
      idCell.textContent = entry.identifier || "-";
      idCell.title = entry.identifier || entry.name;
      row.appendChild(idCell);

      const toggleCell = document.createElement("td");
      toggleCell.classList.add("preferences-col-toggle");
      const toggleLabel = document.createElement("label");
      toggleLabel.classList.add("preferences-toggle");

      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = overrides.get(entry.key) !== false;
      checkbox.disabled = saving;
      checkbox.dataset.appId = entry.key;
      checkbox.addEventListener("change", (event) => {
        const enabled = event.target.checked;
        if (enabled) {
          overrides.delete(entry.key);
          row.classList.remove("preferences-row--disabled");
        } else {
          overrides.set(entry.key, false);
          row.classList.add("preferences-row--disabled");
        }
        scheduleDirtyCompute();
        debug("toggle:change", { appId: entry.key, enabled });
      });

      const caption = document.createElement("span");
      caption.textContent = checkbox.checked ? "Allowed" : "Blocked";
      caption.classList.add("preferences-toggle__caption");

      checkbox.addEventListener("change", () => {
        caption.textContent = checkbox.checked ? "Allowed" : "Blocked";
      });

      toggleLabel.appendChild(checkbox);
      toggleLabel.appendChild(caption);
      toggleCell.appendChild(toggleLabel);
      row.appendChild(toggleCell);

      if (!checkbox.checked) {
        row.classList.add("preferences-row--disabled");
      }

      tableBody.appendChild(row);
    });
  }

  function renderCatalogError(message) {
    tableBody = ensureTableBody(root, tableBody);
    if (!tableBody) {
      return;
    }
    tableBody.innerHTML = "";
    const row = document.createElement("tr");
    row.classList.add("preferences-empty");
    const cell = document.createElement("td");
    cell.colSpan = 3;
    cell.textContent = message;
    row.appendChild(cell);
    tableBody.appendChild(row);
    debug("catalog:error", { message });
  }

  function ingestCatalog(installedApps) {
    if (!Array.isArray(installedApps)) {
      return;
    }
    preferencesState.catalog = installedApps.map((entry) => {
      const identifier = rawAppId(entry) || entry.name || "";
      return {
        key: normalizeId(entry),
        name: entry.name || friendlyLabel(identifier),
        identifier,
      };
    });
    catalog = preferencesState.catalog
      .map((entry) => ({ ...entry, name: entry.name || friendlyLabel(entry.identifier) }))
      .sort((a, b) => a.name.localeCompare(b.name));

    debug("catalog:ingested", { count: catalog.length });
  }

  function applySnapshot(snapshot, installedApps) {
    personalityInput =
      personalityInput && personalityInput.isConnected
        ? personalityInput
        : root?.querySelector("#preferences-personality") || document.querySelector("#preferences-personality");
    debug("personality:applySnapshot", { hasInput: !!personalityInput });
    if (!snapshot) {
      return;
    }
    preferencesState.snapshot = snapshot;
    preferencesState.lastFetchedAt = Date.now();

    if (Array.isArray(installedApps)) {
      ingestCatalog(installedApps);
    }

    if (typeof snapshot.default_personality === "string" && snapshot.default_personality.trim()) {
      debug("personality:default-from-snapshot", { raw: snapshot.default_personality });
      defaultPersonality = snapshot.default_personality.trim();
    } else {
      debug("personality:default-unchanged", { current: defaultPersonality });
    }

    const resolvedPersonality = (() => {
      if (typeof snapshot.personality === "string") {
        const trimmed = snapshot.personality.trim();
        if (trimmed.length) {
          return trimmed;
        }
      }
      return defaultPersonality;
    })();

    initialPersonality = resolvedPersonality;
    if (personalityInput) {
      personalityInput.value = resolvedPersonality;
      debug("personality:input-applied", { value: personalityInput.value });
    }

    initialOverrides = cloneMap(new Map(Object.entries(snapshot.allowlist_overrides || {})));
    overrides = cloneMap(initialOverrides);

    debug("snapshot:applied", {
      personality: resolvedPersonality,
      overrideCount: overrides.size,
      catalogCount: catalog.length,
    });

    renderTable();
    computeDirty();
  }

  async function load(force = false) {
    if (preferencesState.loading && !force) {
      return;
    }
    preferencesState.loading = true;
    setStatus("Loading preferences.", "loading");
    debug("load:start", { force });

    try {
      const catalogPromise = (async () => {
        try {
          return await invoke("refresh_application_catalog");
        } catch (error) {
          console.warn("Failed to refresh application catalog, falling back to cached snapshot", error);
          debug("catalog:refresh-error", { error });
          try {
            return await invoke("get_application_catalog");
          } catch (innerError) {
            console.warn("Failed to read cached application catalog", innerError);
            debug("catalog:fallback-error", { error: innerError });
            return null;
          }
        }
      })();

      const [snapshot, catalogSnapshot] = await Promise.all([
        invoke("get_preferences_snapshot"),
        catalogPromise,
      ]);
      applySnapshot(snapshot, catalogSnapshot?.applications || []);
      debug("load:success", {
        catalogSize: catalogSnapshot?.applications?.length ?? 0,
      });
      setStatus("Preferences loaded", "info");
    } catch (error) {
      console.warn("Failed to load preferences", error);
      debug("load:error", { error });
      setStatus("Failed to load preferences", "error");
      renderCatalogError("Failed to load installed applications.");
    } finally {
      preferencesState.loading = false;
      computeDirty();
    }
  }

  function resetChanges() {
    applySnapshot(preferencesState.snapshot, null);
    setStatus("Changes discarded", "info");
  }

  function overridesPayload() {
    return Array.from(overrides.entries()).map(([appId, enabled]) => ({
      appId,
      enabled,
    }));
  }

  async function save() {
    if (saving) {
      return;
    }
    const payload = {};
    const personalityValue = personalityInput ? personalityInput.value.trim() : "";
    if (personalityValue !== initialPersonality) {
      payload.personality = personalityValue;
    }
    payload.allowlist = overridesPayload();

    if (!payload.allowlist.length && payload.personality === undefined) {
      computeDirty();
      return;
    }

    saving = true;
    setControlsDisabled(true);
    setStatus("Saving preferences.", "loading");
    debug("save:start", payload);
    try {
      const updated = await invoke("update_preferences", { payload });
      applySnapshot(updated, null);
      setStatus("Preferences saved", "success");
      debug("save:success");
    } catch (error) {
      console.error("Failed to update preferences", error);
      debug("save:error", { error });
      setStatus("Failed to update preferences", "error");
    } finally {
      saving = false;
      setControlsDisabled(false);
      computeDirty();
    }
  }

  function handleSearch(event) {
    filterText = event.target.value || "";
    renderTable();
  }

  function restoreDefaults() {
    overrides = new Map();
    renderTable();
    scheduleDirtyCompute();
    setStatus("Allowlist reset to defaults", "info");
  }

  function disableAllApplications() {
    if (!catalog.length) {
      return;
    }
    overrides = new Map(catalog.map((entry) => [entry.key, false]));
    renderTable();
    scheduleDirtyCompute();
    setStatus("All applications disabled", "info");
  }

  if (personalityInput) {
    personalityInput.addEventListener("input", scheduleDirtyCompute);
  }

  if (searchInput) {
    searchInput.addEventListener("input", handleSearch);
    searchInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
      }
    });
  }

  if (restoreButton) {
    restoreButton.addEventListener("click", () => {
      restoreDefaults();
    });
  }

  if (uncheckAllButton) {
    uncheckAllButton.addEventListener("click", () => {
      disableAllApplications();
    });
  }

  if (cancelButton) {
    cancelButton.addEventListener("click", () => {
      resetChanges();
    });
  }

  if (saveButton) {
    saveButton.addEventListener("click", () => {
      save();
    });
  }

  return {
    load,
    onEnter() {
      if (!preferencesState.snapshot) {
        load();
      } else {
        applySnapshot(preferencesState.snapshot, null);
      }
    },
    applyExternalSnapshot(snapshot) {
      if (!snapshot) {
        debug("externalSnapshot:ignored", { reason: "empty" });
        return;
      }
      if (dirty) {
        setStatus("Preferences updated elsewhere. Cancel to reload.", "warning");
        debug("externalSnapshot:ignored", { reason: "dirty" });
        return;
      }
      applySnapshot(snapshot, null);
      setStatus("Preferences reloaded", "info");
      debug("externalSnapshot:applied");
    },
    applyExternalCatalog(snapshot) {
      if (!snapshot || !Array.isArray(snapshot.applications)) {
        debug("externalCatalog:ignored", { reason: "invalid", snapshot });
        return;
      }
      ingestCatalog(snapshot.applications);
      renderTable();
      updateSummaryLabel();
      setStatus("Application catalog updated", "info");
      debug("externalCatalog:applied", { count: snapshot.applications.length });
    },
    isDirty() {
      return dirty;
    },
  };
}
