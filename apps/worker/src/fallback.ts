import { Reply } from "@sprite/shared";

export function buildFallbackReply(query: string): Reply {
  const normalized = query.trim();
  const intents: Reply["intents"] = [];

  if (normalized.length === 0) {
    return {
      reply: "How can I help?",
      intents: [{ type: "converse" }],
    };
  }

  const lower = normalized.toLowerCase();
  if (lower.startsWith("open ") || lower.startsWith("launch ")) {
    const name = normalized.replace(/^\s*(open|launch)\s+/i, "");
    if (name) {
      intents.push({ type: "open_app", app_name: name });
    }
  }

  if (intents.length === 0) {
    intents.push({ type: "converse" });
  }

  return {
    reply: intents.some((intent) => intent.type === "open_app")
      ? `Opening ${normalized}.`
      : `Understood request: "${normalized}"`,
    intents,
  };
}
