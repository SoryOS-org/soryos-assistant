import { useEffect, useState } from "react";
import { useUiStore } from "@/stores/uiStore";
import { ConfigTab } from "./ConfigTab";
import { SecretsTab } from "./SecretsTab";
import { PreferencesTab } from "./PreferencesTab";
import { LogsTab } from "./LogsTab";
import { AboutTab } from "./AboutTab";
import { McpRegistryView } from "@/features/mcp/McpRegistryView";

export type SettingsTab = "config" | "secrets" | "preferences" | "mcp" | "logs" | "about";

const TABS: Array<{ id: SettingsTab; label: string }> = [
  { id: "config", label: "Config" },
  { id: "secrets", label: "Secrets" },
  { id: "preferences", label: "Preferences" },
  { id: "mcp", label: "MCP" },
  { id: "logs", label: "Logs" },
  { id: "about", label: "About" },
];

/** Accepts both v2 tab ids and v1 `panel://navigate` view names. */
function toTab(value: string): SettingsTab | null {
  const normalized = value.toLowerCase();
  if (normalized === "chat") {
    return null;
  }
  const aliases: Record<string, SettingsTab> = {
    config: "config",
    settings: "config",
    secrets: "secrets",
    preferences: "preferences",
    mcp: "mcp",
    logs: "logs",
    about: "about",
  };
  return aliases[normalized] ?? null;
}

/**
 * SettingsView — tabbed settings surface, porting every v1 panel view
 * (config editor, secrets, preferences, MCP registry, logs, about).
 */
export function SettingsView({ initialTab }: { initialTab?: SettingsTab }) {
  const remoteTab = useUiStore((s) => s.settingsTab);
  const [tab, setTab] = useState<SettingsTab>(initialTab ?? toTab(remoteTab) ?? "config");

  // Follow remote navigation (`panel://navigate`) when it targets a tab.
  useEffect(() => {
    const mapped = toTab(remoteTab);
    if (mapped) {
      setTab(mapped);
    }
  }, [remoteTab]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-10 shrink-0 items-center gap-1 border-b border-sprite-border px-3">
        <button
          type="button"
          onClick={() => useUiStore.getState().setMainView("chat")}
          className="mr-2 rounded-md px-2 py-1 text-[12px] text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
        >
          ← Chat
        </button>
        <span className="mr-2 text-[12px] uppercase tracking-wide text-sprite-muted">
          Paramètres
        </span>
        {TABS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            onClick={() => setTab(entry.id)}
            className={`rounded-md px-2 py-1 text-[12px] transition-colors ${
              tab === entry.id
                ? "bg-surface-raised text-sprite-text"
                : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
            }`}
          >
            {entry.label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <div className="mx-auto max-w-2xl">
          {tab === "config" && <ConfigTab />}
          {tab === "secrets" && <SecretsTab />}
          {tab === "preferences" && <PreferencesTab />}
          {tab === "mcp" && <McpRegistryView />}
          {tab === "logs" && <LogsTab />}
          {tab === "about" && <AboutTab />}
        </div>
      </div>
    </div>
  );
}
