export const uiState = {
  state: "Idle",
  timestamp: null,
  vuLevel: null,
};

export const configState = {
  loading: false,
  loaded: false,
  summary: null,
  pendingPatch: {},
  modelCatalog: [],
};

export const chatState = {
  entries: [],
  maxEntries: 100,
};

export const mcpState = {
  entries: [],
  maxEntries: 50,
};

export const logState = {
  lines: [],
  source: null,
  loading: false,
  error: null,
  lastFetchedAt: null,
};

export const preferencesState = {
  snapshot: null,
  catalog: [],
  loading: false,
  lastFetchedAt: null,
};
