import { SidebarView, SidebarRail } from "@/features/sidebar";
import { ChatView } from "@/features/chat";
import { TerminalView } from "@/features/terminal";
import { FilesView } from "@/features/files";
import { BrowserView } from "@/features/browser";
import { SettingsView } from "@/features/settings";
import { VoiceOverlay, VoiceSessionView } from "@/features/voice";
import { useUiStore } from "@/stores/uiStore";
import { LatencyBar } from "./LatencyBar";

/**
 * AppShell — ChatGPT-desktop-like window layout:
 *
 * ┌──────────────┬────────────────────────────┬──────────┐
 * │ Sessions     │  Conversation (centrée)    │ Panel    │
 * │ sidebar      │                            │ (opt.)   │
 * │              ├────────────────────────────┤          │
 * │              │  Composer pill             │          │
 * └──────────────┴────────────────────────────┴──────────┘
 * Collapsed sidebar renders as a 48px icon rail. LatencyBar sits at the
 * bottom of the main column. VoiceOverlay floats above everything.
 */
export function AppShell() {
  const mainView = useUiStore((s) => s.mainView);
  const sidebarOpen = useUiStore((s) => s.sidebarOpen);
  const panelView = useUiStore((s) => s.panelView);
  const panelOpen = useUiStore((s) => s.panelOpen);
  const voiceActive = useUiStore((s) => s.voiceActive);
  const voiceState = useUiStore((s) => s.voiceState);
  const voiceSessionOpen = useUiStore((s) => s.voiceSessionOpen);

  return (
    <div className="flex h-full w-full overflow-hidden bg-sprite-bg">
      {sidebarOpen ? <SidebarView /> : <SidebarRail />}

      <main className="flex min-w-0 flex-1 flex-col">
        {/* Flex column wrapper so children (ChatView/SettingsView) can stretch
            to full height — flex-1 has no effect inside a block parent. */}
        <div className="flex min-h-0 flex-1 flex-col">
          {mainView === "chat" ? <ChatView /> : <SettingsView />}
        </div>
        <LatencyBar />
      </main>

      {panelOpen && (
        <aside className="flex w-[380px] shrink-0 flex-col border-l border-sprite-border bg-sprite-surface">
          {panelView === "terminal" && <TerminalView />}
          {panelView === "files" && <FilesView />}
          {panelView === "browser" && <BrowserView />}
        </aside>
      )}

      {voiceActive && <VoiceOverlay voiceState={voiceState} />}
      {voiceSessionOpen && <VoiceSessionView />}
    </div>
  );
}
