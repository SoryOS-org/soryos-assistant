import { chatState } from "./state.js";

function ensureTimestampMs(timestamp) {
  if (!Number.isFinite(timestamp)) {
    return Date.now();
  }
  return timestamp < 1e12 ? Math.floor(timestamp * 1000) : Math.floor(timestamp);
}

function formatTime(timestamp) {
  if (!timestamp) {
    return "";
  }
  const date = new Date(timestamp);
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function plannerAwareLabel(entry) {
  if (entry.source === "planner") {
    return "Planner";
  }
  return entry.role === "user" ? "User" : "Assistant";
}

function isSameDay(a, b) {
  return (
    a &&
    b &&
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function formatDate(timestamp) {
  if (!timestamp) {
    return "";
  }
  const date = new Date(timestamp);
  return date.toLocaleDateString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function createDateDivider(date) {
  const divider = document.createElement("div");
  divider.classList.add("chat-date-divider");
  const label = document.createElement("span");
  label.classList.add("chat-date-divider__label");
  label.textContent = formatDate(date.getTime());
  divider.appendChild(label);
  return divider;
}

function createEntryElement(entry) {
  const article = document.createElement("article");
  article.classList.add("chat-entry", `chat-entry--${entry.role}`);
  article.dataset.entryId = entry.id;
  if (entry.source) {
    article.classList.add(`chat-entry--${entry.source}`);
  }

  const aside = document.createElement("div");
  aside.classList.add("chat-entry__meta");
  aside.textContent = plannerAwareLabel(entry);
  article.appendChild(aside);

  const bubble = document.createElement("div");
  bubble.classList.add("chat-entry__bubble");
  if (entry.source) {
    bubble.dataset.source = entry.source;
  }
  const rawText = entry.text || "(empty)";
  const normalized =
    entry.source === "planner"
      ? rawText.replace(/^\s*planner[:\-\s]+/i, "").trim() || rawText
      : rawText;
  bubble.textContent = normalized;
  if (entry.context) {
    const contextBlock = document.createElement("div");
    contextBlock.classList.add("chat-entry__context");
    contextBlock.textContent = entry.context;
    bubble.appendChild(contextBlock);
  }
  article.appendChild(bubble);

  const footer = document.createElement("footer");
  footer.classList.add("chat-entry__footer");
  footer.textContent = formatTime(entry.timestamp);

  if (entry.context) {
    const indicator = document.createElement("span");
    indicator.classList.add("chat-entry__context-indicator");
    indicator.textContent = "Context";
    footer.appendChild(indicator);
  }

  if (entry.role === "assistant" && entry.intents?.length) {
    const intents = entry.intents
      .map((intent) => intent.type || intent)
      .filter(Boolean)
      .join(", ");
    if (intents) {
      const intentsSpan = document.createElement("span");
      intentsSpan.classList.add("chat-entry__intents");
      intentsSpan.textContent = `Intents: ${intents}`;
      footer.appendChild(intentsSpan);
    }
  }

  article.appendChild(footer);

  return article;
}

function renderEntries(listElement, placeholder) {
  if (!listElement) {
    return;
  }

  listElement.innerHTML = "";
  const entries = chatState.entries;

  if (!entries.length) {
    if (placeholder) {
      placeholder.hidden = false;
    }
    listElement.classList.remove("has-entries");
    return;
  }

  if (placeholder) {
    placeholder.hidden = true;
  }

  listElement.classList.add("has-entries");
  let lastDate = null;
  entries.forEach((entry) => {
    const entryDate = new Date(entry.timestamp);
    if (!lastDate || !isSameDay(lastDate, entryDate)) {
      listElement.appendChild(createDateDivider(entryDate));
      lastDate = entryDate;
    }
    listElement.appendChild(createEntryElement(entry));
  });

  listElement.scrollTop = listElement.scrollHeight;
}

function hydrateEntriesFromMemory(memory, listElement, placeholder) {
  if (!memory) {
    return;
  }

  if (memory.type === "turns" && Array.isArray(memory.turns)) {
    chatState.entries = memory.turns
      .map((turn) => ({
        id: `${turn.role}-${turn.timestamp}-${Math.random().toString(16).slice(2)}`,
        role: turn.role === "assistant" ? "assistant" : "user",
        text: turn.content ?? "",
        timestamp: ensureTimestampMs(turn.timestamp),
        intents: Array.isArray(turn.intents) ? turn.intents : [],
        source: turn.source || null,
        context: turn.context || null,
      }))
      .sort((a, b) => a.timestamp - b.timestamp);
    renderEntries(listElement, placeholder);
    return;
  }

  if (memory.type === "summary" && memory.summary) {
    const summaryEntry = {
      id: `summary-${Date.now()}-${Math.random().toString(16).slice(2)}`,
      role: "assistant",
      text: memory.summary,
      timestamp: ensureTimestampMs(memory.lastUpdated ?? Date.now()),
      source: "summary",
      intents: [],
      context: null,
    };
    chatState.entries = [summaryEntry];
    renderEntries(listElement, placeholder);
  }
}

function pushEntry(entry, listElement, placeholder) {
  const item = {
    id: `${entry.role}-${Date.now()}-${Math.random().toString(16).slice(2)}`,
    role: entry.role,
    text: entry.text?.trim() || "",
    timestamp: entry.timestamp || Date.now(),
    intents: entry.intents || [],
    source: entry.source || null,
    context: entry.context || null,
  };

  if (item.role === "assistant" && item.source !== "planner" && chatState.entries.length) {
    let insertIndex = chatState.entries.length;
    while (insertIndex > 0 && chatState.entries[insertIndex - 1]?.source === "planner") {
      insertIndex -= 1;
    }
    chatState.entries.splice(insertIndex, 0, item);
  } else {
    chatState.entries.push(item);
  }

  if (chatState.entries.length > (chatState.maxEntries || 100)) {
    chatState.entries.splice(0, chatState.entries.length - chatState.maxEntries);
  }

  renderEntries(listElement, placeholder);
}

export function createChatView({ list, placeholder }) {
  let expandedEntryId = null;
  let contextVisible = true;

  const addTranscript = (payload = {}) => {
    pushEntry(
      {
        role: "user",
        text: payload.text,
        timestamp: Date.now(),
      },
      list,
      placeholder,
    );
  };

  const addAssistant = (payload = {}) => {
    pushEntry(
      {
        role: "assistant",
        text: payload.reply,
        intents: payload.intents,
        timestamp: Date.now(),
        source: payload.source || null,
        context: payload.planner_context || payload.context || null,
      },
      list,
      placeholder,
    );
  };

  const clear = () => {
    chatState.entries = [];
    renderEntries(list, placeholder);
    expandedEntryId = null;
  };

  const hydrateFromMemory = (memory) => {
    hydrateEntriesFromMemory(memory, list, placeholder);
    expandedEntryId = null;
  };

  const collapseAllEntries = () => {
    expandedEntryId = null;
    list?.querySelectorAll(".chat-entry--expanded").forEach((node) => {
      node.classList.remove("chat-entry--expanded");
    });
  };

  const setGlobalMetaVisible = (visible) => {
    if (!list) {
      return;
    }
    list.classList.toggle("chat-log--show-meta", visible);
    if (visible) {
      collapseAllEntries();
    }
  };

  const setContextVisible = (visible) => {
    contextVisible = Boolean(visible);
    if (!list) {
      return;
    }
    list.classList.toggle("chat-log--hide-context", !contextVisible);
  };

  if (list) {
    list.addEventListener("click", (event) => {
      const entryEl = event.target.closest(".chat-entry");
      if (!entryEl || list.classList.contains("chat-log--show-meta")) {
        return;
      }
      const entryId = entryEl.dataset.entryId;
      if (!entryId) {
        return;
      }
      if (expandedEntryId && expandedEntryId !== entryId) {
        const prev = list.querySelector(`.chat-entry[data-entry-id="${expandedEntryId}"]`);
        prev?.classList.remove("chat-entry--expanded");
      }
      if (entryEl.classList.contains("chat-entry--expanded")) {
        entryEl.classList.remove("chat-entry--expanded");
        expandedEntryId = null;
      } else {
        entryEl.classList.add("chat-entry--expanded");
        expandedEntryId = entryId;
      }
    });
  }

  const onEnter = () => {
    if (list) {
      requestAnimationFrame(() => {
        list.scrollTop = list.scrollHeight;
      });
    }
  };

  renderEntries(list, placeholder);

  return {
    addTranscript,
    addAssistant,
    clear,
    hydrateFromMemory,
    setGlobalMetaVisible,
    setContextVisible,
    onEnter,
  };
}
