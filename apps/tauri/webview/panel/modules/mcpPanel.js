export function createMcpPanel({
  layoutEl,
  railEl,
  toggleButton,
  summaryChip,
  progressController,
} = {}) {
  let collapsed = true;

  function updateCollapseUi() {
    if (layoutEl) {
      layoutEl.classList.toggle("is-mcp-collapsed", collapsed);
    }
    if (toggleButton) {
      toggleButton.textContent = collapsed ? "Show MCP" : "Hide MCP";
    }
    if (railEl) {
      railEl.hidden = collapsed;
    }
  }

  function applySummary() {
    if (!summaryChip || !progressController?.getSummary) {
      return;
    }
    const summary = progressController.getSummary();
    const parts = [];
    if (summary.running) {
      parts.push(`${summary.running} running`);
    }
    if (summary.crashed) {
      parts.push(`${summary.crashed} error${summary.crashed > 1 ? "s" : ""}`);
    }
    if (summary.queued) {
      parts.push(`${summary.queued} queued`);
    }
    if (!parts.length && summary.completed) {
      parts.push(`${summary.completed} completed`);
    }
    summaryChip.textContent = parts.length ? `MCP: ${parts.join(" | ")}` : "MCP: idle";
  }

  function toggle() {
    collapsed = !collapsed;
    updateCollapseUi();
  }

  function init() {
    if (toggleButton) {
      toggleButton.addEventListener("click", () => toggle());
    }
    updateCollapseUi();
    applySummary();
  }

  return {
    init,
    toggle,
    applySummary,
  };
}
