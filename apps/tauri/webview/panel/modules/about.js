export function createAboutMetadata({ buildEl, runtimeEl, osEl, invoke }) {
  function hydrateOsLabel() {
    if (!osEl) return;

    let platform =
      navigator.userAgentData?.platform ||
      navigator.platform ||
      navigator.userAgent ||
      "Unknown platform";

    platform = String(platform).trim();
    if (!platform) {
      platform = "Unknown platform";
    }
    osEl.textContent = platform;
  }

  function hydrateFallback() {
    if (buildEl && !buildEl.textContent.trim()) {
      buildEl.textContent = "Version information unavailable";
    }
    if (runtimeEl && !runtimeEl.textContent.trim()) {
      runtimeEl.textContent = "Runtime details unavailable";
    }
    hydrateOsLabel();
  }

  async function load() {
    if (!buildEl && !runtimeEl && !osEl) {
      return;
    }

    if (!invoke) {
      hydrateFallback();
      return;
    }

    try {
      const metadata = await invoke("get_about_metadata");
      if (buildEl) {
        const parts = [];
        if (metadata?.version) {
          parts.push(`v${metadata.version}`);
        }
        if (metadata?.commit && metadata.commit !== "unknown") {
          parts.push(metadata.commit);
        }
        if (metadata?.build_timestamp) {
          const ts = Number(metadata.build_timestamp);
          if (Number.isFinite(ts) && ts > 0) {
            const date = new Date(ts * 1000);
            parts.push(date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }));
          }
        }
        buildEl.textContent = parts.length ? parts.join(" | ") : "Version information unavailable";
      }

      if (runtimeEl) {
        const runtimeParts = [];
        if (metadata?.tauri) {
          runtimeParts.push(metadata.tauri);
        }
        if (metadata?.target) {
          runtimeParts.push(metadata.target);
        }
        runtimeEl.textContent = runtimeParts.length
          ? runtimeParts.join(" | ")
          : "Runtime details unavailable";
      }

      hydrateOsLabel();
    } catch (error) {
      console.warn("Failed to load about metadata", error);
      hydrateFallback();
    }
  }

  return {
    load,
  };
}
