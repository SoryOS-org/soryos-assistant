function updateStatusLabel(element, text, tone = "info") {
  if (!element) return;
  element.textContent = text;
  element.dataset.state = tone;
}

function formatSecretHint(hint) {
  const full = hint || "";
  if (!full) {
    return { full: "", compact: "" };
  }
  if (full.length <= 12) {
    return { full, compact: full };
  }
  return {
    full,
    compact: `${full.slice(0, 4)}.${full.slice(-4)}`,
  };
}

export function createSecretsController({
  entries = [],
  statusLabel,
  invoke,
  onStatusChange,
} = {}) {
  let cachedStatus = {};
  function setLoading(entry) {
    if (entry?.pill) {
      entry.pill.textContent = "Loading...";
      entry.pill.dataset.state = "loading";
    }
    if (entry?.hint) {
      entry.hint.textContent = "";
    }
    if (entry?.hintInput) {
      entry.hintInput.value = "";
      entry.hintInput.placeholder = "Loading...";
    }
    if (entry?.setButton) {
      entry.setButton.disabled = true;
    }
    if (entry?.clearButton) {
      entry.clearButton.disabled = true;
    }
  }

  function applyEntryStatus(entry, fieldStatus) {
    if (!entry?.row || !entry?.pill) {
      return;
    }
    if (!fieldStatus) {
      entry.pill.textContent = "Unknown";
      entry.pill.dataset.state = "warning";
      if (entry.hint) {
        entry.hint.textContent = "";
      }
      if (entry.hintInput) {
        entry.hintInput.value = "";
        entry.hintInput.placeholder = "Unavailable";
      }
      return;
    }

    const { has_key: hasKey, hint: storedHint } = fieldStatus;
    const hintParts = formatSecretHint(storedHint);
    if (hasKey) {
      entry.pill.textContent = "Present";
      entry.pill.dataset.state = "ok";
      if (entry.hint) {
        entry.hint.textContent = "";
      }
      if (entry.hintInput) {
        entry.hintInput.value = hintParts.full;
        entry.hintInput.placeholder = hintParts.full ? "" : "Stored";
        entry.hintInput.title = hintParts.full || "";
      }
      if (entry.clearButton) {
        entry.clearButton.disabled = false;
      }
      if (entry.setButton) {
        entry.setButton.disabled = false;
      }
    } else {
      entry.pill.textContent = "Missing";
      entry.pill.dataset.state = "error";
      if (entry.hint) {
        entry.hint.textContent = "";
      }
      if (entry.hintInput) {
        entry.hintInput.value = "";
        entry.hintInput.placeholder = "Not set";
      }
      if (entry.clearButton) {
        entry.clearButton.disabled = true;
      }
      if (entry.setButton) {
        entry.setButton.disabled = false;
      }
    }
  }

  function applyStatus(status) {
    if (!entries.length) {
      return;
    }

    if (status && typeof status === "object") {
      cachedStatus = { ...cachedStatus, ...status };
    }

    entries.forEach((entry) => {
      applyEntryStatus(entry, cachedStatus?.[entry.key]);
    });

    updateStatusLabel(statusLabel, "Secrets loaded", "info");
    if (onStatusChange) {
      onStatusChange(cachedStatus);
    }
  }

  async function loadStatus() {
    if (!invoke) {
      return;
    }
    entries.forEach(setLoading);
    updateStatusLabel(statusLabel, "Loading secrets...", "loading");
    try {
      const status = await invoke("get_secrets_status");
      applyStatus(status);
      updateStatusLabel(statusLabel, "Secrets ready", "info");
    } catch (error) {
      console.warn("Failed to load secrets status", error);
      updateStatusLabel(statusLabel, "Failed to load secrets", "error");
    }
  }

  function bindActions() {
    entries.forEach((entry) => {
      if (entry.setButton && invoke) {
        entry.setButton.addEventListener("click", async () => {
          const value = window.prompt(entry.prompt || "Enter access key:", "");
          if (value == null) {
            return;
          }
          const trimmed = value.trim();
          if (!trimmed) {
            updateStatusLabel(statusLabel, "Access key cannot be empty", "error");
            return;
          }
          entry.setButton.disabled = true;
          if (entry.clearButton) {
            entry.clearButton.disabled = true;
          }
          setLoading(entry);
          updateStatusLabel(statusLabel, entry.savingLabel || "Saving key...", "loading");
          try {
            const status = await invoke(entry.setCommand, { value: trimmed });
            applyStatus({ [entry.key]: status });
            updateStatusLabel(statusLabel, entry.savedLabel || "Key saved", "success");
          } catch (error) {
            console.error(`Failed to save ${entry.key}`, error);
            updateStatusLabel(statusLabel, "Failed to save key", "error");
          } finally {
            entry.setButton.disabled = false;
          }
        });
      }

      if (entry.clearButton && invoke) {
        entry.clearButton.addEventListener("click", async () => {
          if (entry.clearButton.disabled) {
            return;
          }
          const confirmClear = window.confirm(
            entry.clearConfirm || "Remove the stored key?",
          );
          if (!confirmClear) {
            return;
          }
          if (entry.setButton) {
            entry.setButton.disabled = true;
          }
          entry.clearButton.disabled = true;
          setLoading(entry);
          updateStatusLabel(statusLabel, entry.clearingLabel || "Clearing key...", "loading");
          try {
            const status = await invoke(entry.clearCommand);
            applyStatus({ [entry.key]: status });
            updateStatusLabel(statusLabel, entry.clearedLabel || "Key cleared", "success");
          } catch (error) {
            console.error(`Failed to clear ${entry.key}`, error);
            updateStatusLabel(statusLabel, "Failed to clear key", "error");
            entry.clearButton.disabled = false;
          } finally {
            if (entry.setButton) {
              entry.setButton.disabled = false;
            }
          }
        });
      }
    });
  }

  return {
    loadStatus,
    applyStatus,
    setLoading,
    bindActions,
  };
}
