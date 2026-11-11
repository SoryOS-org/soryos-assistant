import { useState, type ReactNode } from "react";
import { useChatStore } from "@/stores/chatStore";
import { useUiStore } from "@/stores/uiStore";
import { resetWorkerSession } from "@/lib/tauri";

/**
 * SidebarView — ChatGPT-style session sidebar:
 * collapse button, "Nouvelle discussion", search over the session history,
 * session rows (click to open, hover to delete), and panels at the bottom.
 */
export function SidebarView() {
  const [filter, setFilter] = useState("");

  const sessions = useChatStore((s) => s.sessions);
  const activeId = useChatStore((s) => s.activeId);
  const newSession = useChatStore((s) => s.newSession);
  const selectSession = useChatStore((s) => s.selectSession);
  const deleteSession = useChatStore((s) => s.deleteSession);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const setMainView = useUiStore((s) => s.setMainView);
  const panelView = useUiStore((s) => s.panelView);
  const setPanelView = useUiStore((s) => s.setPanelView);
  const togglePanel = useUiStore((s) => s.togglePanel);

  const needle = filter.trim().toLowerCase();
  const visible = sessions
    .filter((s) => !needle || s.title.toLowerCase().includes(needle))
    .sort((a, b) => b.updatedAt - a.updatedAt);

  return (
    <nav className="flex w-64 shrink-0 flex-col bg-sprite-surface">
      {/* Top bar: collapse + new chat (ChatGPT layout) */}
      <div className="flex items-center justify-between p-2">
        <button
          type="button"
          onClick={toggleSidebar}
          title="Masquer la barre latérale"
          aria-label="Masquer la barre latérale"
          className="flex h-9 w-9 items-center justify-center rounded-lg text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M2 3.5h12v1.2H2V3.5Zm0 4h12v1.2H2V7.5Zm0 4h8v1.2H2v-1.2Z" />
          </svg>
        </button>
        <button
          type="button"
          onClick={newSession}
          title="Nouvelle discussion"
          aria-label="Nouvelle discussion"
          className="flex h-9 w-9 items-center justify-center rounded-lg text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M12.1 1.9a1.6 1.6 0 0 1 2.3 2.3l-8.2 8.2-3.2.9.9-3.2 8.2-8.2Z" />
          </svg>
        </button>
      </div>

      {/* Search */}
      <div className="px-2 pb-2">
        <div className="flex items-center gap-1.5 rounded-lg bg-sprite-bg px-2 py-1.5">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className="shrink-0 text-sprite-muted">
            <path d="M10.7 9.3h-.6l-.2-.2a4.5 4.5 0 1 0-.8.8l.2.2v.6L13 14.4 14.4 13l-3.7-3.7Zm-4.2 0a3 3 0 1 1 0-6 3 3 0 0 1 0 6Z" />
          </svg>
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Rechercher"
            data-selectable="true"
            className="min-w-0 flex-1 bg-transparent text-[12.5px] text-sprite-text outline-none placeholder:text-sprite-muted"
          />
        </div>
      </div>

      {/* Session history (ChatGPT-style rows) */}
      <div className="min-h-0 flex-1 overflow-y-auto px-2">
        <p className="px-2 pb-1 pt-2 text-[11px] text-sprite-muted">Discussions</p>
        {visible.length === 0 ? (
          <p className="px-2 py-3 text-[12px] text-sprite-muted">Aucune discussion</p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {visible.map((session) => (
              <li key={session.id} className="session-row relative">
                <button
                  type="button"
                  onClick={() => selectSession(session.id)}
                  className={`flex w-full items-center gap-2 rounded-lg px-2 py-2 pr-8 text-left text-[13px] transition-colors ${
                    session.id === activeId
                      ? "bg-surface-raised text-sprite-text"
                      : "text-sprite-text hover:bg-surface-raised/70"
                  }`}
                >
                  <span className="min-w-0 flex-1 truncate">{session.title}</span>
                </button>
                <button
                  type="button"
                  onClick={() => deleteSession(session.id)}
                  title="Supprimer la discussion"
                  aria-label="Supprimer la discussion"
                  className="session-delete absolute right-1.5 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-md text-sprite-muted hover:bg-sprite-danger/20 hover:text-sprite-danger"
                >
                  <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                    <path d="M4 4.7 4.7 4l3.3 3.3L11.3 4l.7.7L8.7 8l3.3 3.3-.7.7L8 8.7 4.7 12l-.7-.7L7.3 8 4 4.7Z" />
                  </svg>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {/* Bottom: reset, panels + settings (ChatGPT-like footer) */}
      <div className="border-t border-sprite-border p-2">
        <SidebarFooterButton
          label="Réinitialiser la discussion"
          onClick={() => {
            useChatStore.getState().clearCurrent();
            void resetWorkerSession();
          }}
        />
        <SidebarFooterButton
          label="Paramètres"
          onClick={() => setMainView("settings")}
        />
        <div className="mt-1 flex flex-col gap-0.5">
          <span className="px-2 pb-1 text-[10.5px] uppercase tracking-wider text-sprite-muted">
            Panneaux
          </span>
          <PanelButton
            label="Terminal"
            active={panelView === "terminal"}
            onClick={() => setPanelView("terminal")}
            onToggle={togglePanel}
          />
          <PanelButton
            label="Fichiers"
            active={panelView === "files"}
            onClick={() => setPanelView("files")}
            onToggle={togglePanel}
          />
          <PanelButton
            label="Navigateur"
            active={panelView === "browser"}
            onClick={() => setPanelView("browser")}
            onToggle={togglePanel}
          />
        </div>
      </div>
    </nav>
  );
}

/** Collapsed 48px rail shown when the sidebar is hidden. */
export function SidebarRail() {
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const newSession = useChatStore((s) => s.newSession);

  return (
    <nav className="flex w-12 shrink-0 flex-col items-center gap-1 bg-sprite-surface py-2">
      <RailButton
        label="Afficher la barre latérale"
        onClick={toggleSidebar}
        icon={
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M2 3.5h12v1.2H2V3.5Zm0 4h12v1.2H2V7.5Zm0 4h8v1.2H2v-1.2Z" />
          </svg>
        }
      />
      <RailButton
        label="Nouvelle discussion"
        onClick={newSession}
        icon={
          <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M12.1 1.9a1.6 1.6 0 0 1 2.3 2.3l-8.2 8.2-3.2.9.9-3.2 8.2-8.2Z" />
          </svg>
        }
      />
    </nav>
  );
}

function RailButton({
  label,
  onClick,
  icon,
}: {
  label: string;
  onClick: () => void;
  icon: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="flex h-9 w-9 items-center justify-center rounded-lg text-sprite-muted transition-colors hover:bg-surface-raised hover:text-sprite-text"
    >
      {icon}
    </button>
  );
}

function SidebarFooterButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full items-center gap-2 rounded-lg px-2 py-2 text-left text-[13px] text-sprite-text transition-colors hover:bg-surface-raised/70"
    >
      <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className="text-sprite-muted">
        <path d="M8 2a2.5 2.5 0 1 1 0 5 2.5 2.5 0 0 1 0-5ZM2.5 12.5c0-2.2 2.5-3.5 5.5-3.5s5.5 1.3 5.5 3.5V14h-11v-1.5Z" />
      </svg>
      {label}
    </button>
  );
}

function PanelButton({
  label,
  active,
  onClick,
  onToggle,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onToggle}
      className={`flex w-full items-center justify-between rounded-lg px-2 py-1.5 text-left text-[12.5px] transition-colors ${
        active ? "text-sprite-text" : "text-sprite-muted hover:bg-surface-raised/60 hover:text-sprite-text"
      }`}
    >
      <span onClick={onClick}>{label}</span>
      <span className="text-[10px] text-sprite-muted" aria-hidden="true">
        ⇄
      </span>
    </button>
  );
}
