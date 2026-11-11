/**
 * Conversation store — sessions + entries.
 *
 * Ports the v1 `chatView.js` state logic (`hydrateEntriesFromMemory`,
 * `pushEntry` insertion semantics, 100-entry cap) and adds ChatGPT-style
 * session management: multiple conversations, auto-titled from the first
 * user message, persisted to localStorage so the sidebar history survives
 * reloads.
 */
import { create } from "zustand";
import type {
  ChatEntry,
  ChatIntent,
  ConversationMemory,
  SttResultPayload,
} from "@/types";

const MAX_ENTRIES = 100;
const STORAGE_KEY = "sprite.chat.sessions.v2";

export interface ChatSession {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  entries: ChatEntry[];
}

function ensureTimestampMs(timestamp: number | null | undefined): number {
  if (typeof timestamp !== "number" || !Number.isFinite(timestamp)) {
    return Date.now();
  }
  return timestamp < 1e12 ? Math.floor(timestamp * 1000) : Math.floor(timestamp);
}

let counter = 0;
function nextId(prefix: string): string {
  counter += 1;
  return `${prefix}-${Date.now().toString(36)}-${counter}`;
}

function newSession(id: string): ChatSession {
  return { id, title: "Nouvelle discussion", createdAt: Date.now(), updatedAt: Date.now(), entries: [] };
}

/* localStorage persistence (best-effort, browser + Tauri webview alike). */
function loadPersisted(): { sessions: ChatSession[]; activeId: string } | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw) as {
      sessions?: ChatSession[];
      activeId?: string;
    };
    if (!Array.isArray(parsed.sessions) || parsed.sessions.length === 0) {
      return null;
    }
    const sessions = parsed.sessions.filter(
      (s) => s && typeof s.id === "string" && Array.isArray(s.entries),
    );
    if (!sessions.length) {
      return null;
    }
    const activeId =
      typeof parsed.activeId === "string" && sessions.some((s) => s.id === parsed.activeId)
        ? parsed.activeId
        : sessions[0].id;
    return { sessions, activeId };
  } catch {
    return null;
  }
}

function persist(sessions: ChatSession[], activeId: string): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ sessions, activeId }));
  } catch {
    // Storage may be unavailable; the UI keeps working in memory.
  }
}

const persisted = typeof localStorage !== "undefined" ? loadPersisted() : null;
const bootstrapId = persisted?.activeId ?? nextId("session");

interface ChatState {
  sessions: ChatSession[];
  activeId: string;
  /** Global "show Details" toggle (v1 `chat-log--show-meta`). */
  showMeta: boolean;
  /** Global "Show Context" toggle (v1 `chat-log--hide-context` inverse). */
  contextVisible: boolean;
  /** True while a `process_user_request` turn is running. */
  pending: boolean;
  /** Last assistant arrival time, used by the composer to avoid echo. */
  lastAssistantAtMs: number;

  newSession: () => void;
  selectSession: (id: string) => void;
  deleteSession: (id: string) => void;
  hydrateFromMemory: (memory: ConversationMemory) => void;
  /** Add a final user transcript (`assistant://stt-final` / composer send). */
  addTranscript: (payload: { text?: string; timestamp?: number }) => void;
  /** Add an assistant reply (`assistant://stt-result`). */
  addAssistant: (payload: SttResultPayload) => void;
  clearCurrent: () => void;

  setGlobalMetaVisible: (visible: boolean) => void;
  setContextVisible: (visible: boolean) => void;
  setPending: (pending: boolean) => void;
}

export const useChatStore = create<ChatState>((set, get) => {
  /** Mutate the active session and bump its `updatedAt`. */
  function updateActive(
    fn: (session: ChatSession) => Partial<ChatSession> | null,
  ): void {
    set((s) => {
      const sessions = s.sessions.map((session) => {
        if (session.id !== s.activeId) {
          return session;
        }
        const patch = fn(session);
        if (!patch) {
          return session;
        }
        return { ...session, ...patch, updatedAt: Date.now() };
      });
      return { sessions };
    });
  }

  /** v1 `pushEntry` cap + planner-aware insertion of assistant entries. */
  function appendEntry(entry: ChatEntry): void {
    updateActive((session) => {
      const entries = [...session.entries];
      if (entry.role === "assistant" && entry.source !== "planner" && entries.length) {
        let insertIndex = entries.length;
        while (insertIndex > 0 && entries[insertIndex - 1]?.source === "planner") {
          insertIndex -= 1;
        }
        entries.splice(insertIndex, 0, entry);
      } else {
        entries.push(entry);
      }
      if (entries.length > MAX_ENTRIES) {
        entries.splice(0, entries.length - MAX_ENTRIES);
      }
      return { entries };
    });
  }

  return {
    sessions: persisted?.sessions ?? [newSession(bootstrapId)],
    activeId: bootstrapId,
    showMeta: false,
    contextVisible: true,
    pending: false,
    lastAssistantAtMs: 0,

    newSession: () => {
      const id = nextId("session");
      set((s) => ({ sessions: [...s.sessions, newSession(id)], activeId: id }));
    },

    selectSession: (id) => {
      if (get().sessions.some((s) => s.id === id)) {
        set({ activeId: id });
      }
    },

    deleteSession: (id) => {
      set((s) => {
        const remaining = s.sessions.filter((session) => session.id !== id);
        if (!remaining.length) {
          const fresh = newSession(nextId("session"));
          return { sessions: [fresh], activeId: fresh.id };
        }
        return {
          sessions: remaining,
          activeId: s.activeId === id ? remaining[remaining.length - 1].id : s.activeId,
        };
      });
    },

    hydrateFromMemory: (memory) => {
      let entries: ChatEntry[] = [];

      if (memory?.type === "turns" && Array.isArray(memory.turns)) {
        entries = memory.turns
          .map((turn): ChatEntry => ({
            id: nextId(turn.role === "assistant" ? "assistant" : "user"),
            role: turn.role === "assistant" ? "assistant" : "user",
            text: turn.content ?? "",
            timestamp: ensureTimestampMs(turn.timestamp),
            intents: Array.isArray(turn.intents) ? (turn.intents as ChatIntent[]) : [],
            source: turn.source || null,
            context: turn.context || null,
          }))
          .sort((a, b) => a.timestamp - b.timestamp);
      } else if (memory?.type === "summary" && memory.summary) {
        entries = [
          {
            id: nextId("summary"),
            role: "assistant",
            text: memory.summary,
            timestamp: ensureTimestampMs(memory.lastUpdated ?? Date.now()),
            source: "summary",
            intents: [],
            context: null,
          },
        ];
      }

      updateActive(() => ({ entries }));
    },

    addTranscript: ({ text, timestamp }) => {
      const clean = (text ?? "").trim();
      if (!clean) {
        return;
      }
      const entry: ChatEntry = {
        id: nextId("user"),
        role: "user",
        text: clean,
        timestamp: timestamp ?? Date.now(),
        intents: [],
        source: null,
        context: null,
      };
      updateActive((session) => {
        const title =
          session.entries.length === 0
            ? clean.length > 42
              ? `${clean.slice(0, 42)}…`
              : clean
            : session.title;
        const entries = [...session.entries, entry];
        if (entries.length > MAX_ENTRIES) {
          entries.splice(0, entries.length - MAX_ENTRIES);
        }
        return { entries, title };
      });
    },

    addAssistant: (payload) => {
      const entry: ChatEntry = {
        id: nextId("assistant"),
        role: "assistant",
        text: (payload.reply ?? "").trim(),
        timestamp: Date.now(),
        intents: Array.isArray(payload.intents) ? (payload.intents as ChatIntent[]) : [],
        source: payload.source || null,
        context: payload.planner_context || payload.context || null,
      };
      if (!entry.text) {
        return;
      }
      set({ lastAssistantAtMs: Date.now() });
      appendEntry(entry);
    },

    clearCurrent: () => {
      updateActive((session) => ({ ...session, entries: [], title: "Nouvelle discussion" }));
    },

    setGlobalMetaVisible: (visible) => set({ showMeta: Boolean(visible) }),
    setContextVisible: (visible) => set({ contextVisible: Boolean(visible) }),
    setPending: (pending) => set({ pending: Boolean(pending) }),
  };
});

// Persist sessions on every change (ChatGPT-style history that survives
// reloads).
useChatStore.subscribe((state) => {
  persist(state.sessions, state.activeId);
});

/**
 * Insertion semantics ported from v1 `pushEntry`: an assistant entry that is
 * NOT from the planner is inserted BEFORE trailing planner entries, so the
 * final reply lands just under the user's turn.
 */
// (Handled inline in addAssistant's session append; planner reordering is a
// v1 rendering concern preserved by MessageList's planner-aware styling.)

/** Planner-aware label (v1 `plannerAwareLabel`). */
export function plannerAwareLabel(entry: ChatEntry): string {
  if (entry.source === "planner") {
    return "Planner";
  }
  return entry.role === "user" ? "User" : "Assistant";
}
