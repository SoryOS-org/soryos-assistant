import { useChatStore } from "@/stores/chatStore";
import { useMcpProgressStore } from "@/stores/mcpStore";
import { useUiStore } from "@/stores/uiStore";
import { panelHide } from "@/lib/tauri";
import { MessageList } from "./MessageList";
import { Composer } from "./Composer";
import { PillsRow, McpSummaryChip, ControlChips } from "./chatStatus";
import { McpProgressRail } from "@/features/mcp/McpProgressRail";
import { IconButton } from "@/ui/IconButton";
/**
 * ChatView — ChatGPT-style conversation surface, ported from the v1 panel:
 * top bar (collapse sidebar, "Nouvelle discussion", session title, MCP chip,
 * panel toggle), status pills, Show Details/Show Context (v1 `chatControls`),
 * the MCP progress rail (v1 `mcpPanel`) and the transcript.
 */
export function ChatView() {
  const session = useChatStore((s) => s.sessions.find((x) => x.id === s.activeId));
  const setGlobalMetaVisible = useChatStore((s) => s.setGlobalMetaVisible);
  const setContextVisible = useChatStore((s) => s.setContextVisible);
  const togglePanel = useUiStore((s) => s.togglePanel);
  const mcpRailVisible = useUiStore((s) => s.mcpRailVisible);

  // ChatGPT behavior: when the conversation is empty, the welcome block and
  // the composer are centered together; otherwise the transcript fills the
  // column and the composer sits at the bottom.
  const isEmpty = (session?.entries.length ?? 0) === 0;
  // The MCP rail only appears when there is actual tool-call activity
  // (never on a plain conversation).
  const mcpEntries = useMcpProgressStore((s) => s.entries);
  // The rail shows only with actual tool-call activity AND when the user has
  // not hidden it manually (v1 "Show/Hide MCP").
  const showMcpRail = mcpEntries.length > 0 && mcpRailVisible;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* Top bar (ChatGPT-style): session title + controls only. The sidebar
          toggle and "new discussion" buttons live in the sidebar/rail only,
          to keep this bar uncluttered. */}
      <header className="flex h-11 shrink-0 items-center justify-between px-3">
        <div className="flex items-center gap-1">
          <span className="max-w-[320px] truncate text-[13.5px] font-medium text-sprite-text">
            {session?.title ?? "Sprite"}
          </span>
        </div>

        <div className="flex items-center gap-2">
          <McpSummaryChip />
          <ControlChips
            onMeta={(active) => setGlobalMetaVisible(active)}
            onContext={(active) => setContextVisible(active)}
          />
          <IconButton label="Masquer le panneau" onClick={() => void panelHide()}>
            <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <rect x="2" y="7" width="12" height="1.6" rx="0.8" />
            </svg>
          </IconButton>
          <IconButton label="Basculer le panneau droit" onClick={togglePanel}>
            <svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
              <rect x="0" y="2" width="16" height="12" rx="2" opacity="0.4" />
              <rect x="9" y="2" width="7" height="12" rx="2" />
            </svg>
          </IconButton>
        </div>
      </header>

      <PillsRow />

      {isEmpty ? (
        /* Welcome state: greeting + composer centered, ChatGPT-style */
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto px-4">
          <div className="flex w-full max-w-3xl flex-col items-center gap-6 pb-16">
            <span className="flex h-14 w-14 items-center justify-center rounded-full bg-surface-raised text-[22px]">
              ✳️
            </span>
            <h2 className="text-[22px] font-semibold text-sprite-text">
              Comment puis-je vous aider&nbsp;?
            </h2>
            <p className="max-w-md text-center text-[13px] text-sprite-muted">
              Parlez à Sprite (micro/Ctrl+Espace dans la fenêtre sprite) ou écrivez
              ci-dessous.
            </p>
            <div className="w-full pt-4">
              <Composer />
            </div>
          </div>
        </div>
      ) : (
        <>
          {/* Transcript + MCP rail (rail only while tool calls are tracked) */}
          <div className="flex min-h-0 flex-1">
            <MessageList />
            {showMcpRail ? <McpProgressRail /> : null}
          </div>

          {/* Input pinned at the bottom while the conversation grows */}
          <Composer />
        </>
      )}
    </div>
  );
}
