import { useEffect, useState } from "react";
import { getAboutMetadata, openExternalUrl } from "@/lib/tauri";
import type { AboutMetadata } from "@/types";

/**
 * AboutTab — port of v1 `about.js`: build/runtime metadata from
 * `get_about_metadata` + the OS label.
 */
export function AboutTab() {
  const [build, setBuild] = useState("Version information unavailable");
  const [runtime, setRuntime] = useState("Runtime details unavailable");
  const [os] = useState(hydrateOsLabel());

  useEffect(() => {
    let disposed = false;
    void getAboutMetadata()
      .then((metadata: AboutMetadata | null) => {
        if (disposed || !metadata) {
          return;
        }
        const parts: string[] = [];
        if (metadata.version) {
          parts.push(`v${metadata.version}`);
        }
        if (metadata.commit && metadata.commit !== "unknown") {
          parts.push(metadata.commit);
        }
        if (metadata.build_timestamp) {
          const ts = Number(metadata.build_timestamp);
          if (Number.isFinite(ts) && ts > 0) {
            parts.push(
              new Date(ts * 1000).toLocaleString(undefined, {
                dateStyle: "medium",
                timeStyle: "short",
              }),
            );
          }
        }
        setBuild(parts.length ? parts.join(" | ") : "Version information unavailable");

        const runtimeParts: string[] = [];
        if (metadata.tauri) {
          runtimeParts.push(metadata.tauri);
        }
        if (metadata.target) {
          runtimeParts.push(metadata.target);
        }
        setRuntime(
          runtimeParts.length ? runtimeParts.join(" | ") : "Runtime details unavailable",
        );
      })
      .catch(() => {
        // Keep the fallback labels.
      });
    return () => {
      disposed = true;
    };
  }, []);

  return (
    <div className="flex flex-col gap-2 text-[12.5px]" data-selectable="true">
      <div>
        <span className="text-sprite-muted">Build: </span>
        <span className="text-sprite-text">{build}</span>
      </div>
      <div>
        <span className="text-sprite-muted">Runtime: </span>
        <span className="text-sprite-text">{runtime}</span>
      </div>
      <div>
        <span className="text-sprite-muted">OS: </span>
        <span className="text-sprite-text">{os}</span>
      </div>

      {/* v1 "Highlights" card (panel/index.html:712-718). */}
      <section className="mt-3 rounded-md border border-sprite-border bg-sprite-surface p-3">
        <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-sprite-muted">
          Highlights
        </h3>
        <ul className="flex flex-col gap-1.5 text-[11.5px] text-sprite-muted">
          <li>
            <strong className="text-sprite-text">Speech pipeline</strong> — Wake word + PTT
            capture into Whisper; preload banners keep STT asset downloads visible.
          </li>
          <li>
            <strong className="text-sprite-text">MCP host</strong> — Built-in stdio host with
            lifecycle telemetry and registry view; edit <code>mcp.config.json</code> from here.
          </li>
          <li>
            <strong className="text-sprite-text">Control panel</strong> — Chat with MCP rail,
            config editor, preferences (personality + allowlist), latency HUD, and log tail.
          </li>
          <li>
            <strong className="text-sprite-text">Durable state</strong> — Preferences,
            conversation memory, and app allowlists persist per machine in the worker state
            store.
          </li>
        </ul>
      </section>

      {/* v1 useful links (panel/index.html:724-727), opened through the
          shell-open wrapper instead of raw anchors. */}
      <section className="rounded-md border border-sprite-border bg-sprite-surface p-3">
        <h3 className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-sprite-muted">
          Useful links
        </h3>
        <ul className="flex flex-col gap-1.5 text-[11.5px]">
          {USEFUL_LINKS.map((link) => (
            <li key={link.url}>
              <button
                type="button"
                onClick={() => void openExternalUrl(link.url)}
                className="text-sprite-accent underline-offset-2 hover:underline"
              >
                {link.label}
              </button>
            </li>
          ))}
        </ul>
      </section>

      <p className="mt-3 text-[11.5px] text-sprite-muted">
        Sprite Assistant — v2 desktop UI (React + TypeScript), backed by the same
        Rust core as the legacy panel.
      </p>
    </div>
  );
}

/** Exact v1 links (panel/index.html:724-727). */
const USEFUL_LINKS: Array<{ label: string; url: string }> = [
  { label: "Source repository", url: "https://github.com/tulayha/sprite-assistant" },
  {
    label: "Architecture overview",
    url: "https://github.com/tulayha/sprite-assistant/blob/main/ARCHITECTURE.md",
  },
  {
    label: "Desktop guide",
    url: "https://github.com/tulayha/sprite-assistant/blob/main/apps/tauri/README.md",
  },
  {
    label: "Worker guide",
    url: "https://github.com/tulayha/sprite-assistant/blob/main/apps/worker/README.md",
  },
];

function hydrateOsLabel(): string {
  const platform =
    // Typed loosely on purpose: userAgentData is not in the TS DOM lib yet.
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ||
    navigator.platform ||
    navigator.userAgent ||
    "Unknown platform";
  const normalized = String(platform).trim();
  return normalized || "Unknown platform";
}
