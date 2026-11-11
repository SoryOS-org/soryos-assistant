export function exposeAssistantApi(invoke) {
  if (!invoke || window.spriteAssistant) {
    return;
  }

  const assistantApi = {
    async launchApp(query, options = {}) {
      const { strict = false, refreshCatalog = false, focus = false } = options;
      return invoke("handle_intent", {
        request: {
          type: "launch_application",
          query,
          strict,
          refresh_catalog: refreshCatalog,
          focus,
        },
      });
    },
    async request(query) {
      if (typeof query !== "string" || !query.trim()) {
        throw new TypeError("query must be a non-empty string");
      }

      try {
        return await invoke("process_user_request", { userQuery: query });
      } catch (error) {
        console.error("Failed to process assistant request", error);
        throw error;
      }
    },
    async launchIntents(requests = []) {
      return invoke("handle_intents_batch", { requests });
    },
    async refreshCatalog() {
      return invoke("refresh_application_catalog");
    },
    async catalogSnapshot() {
      return invoke("get_application_catalog");
    },
  };

  Object.defineProperty(window, "spriteAssistant", {
    value: Object.freeze(assistantApi),
    configurable: false,
    writable: false,
  });
}
