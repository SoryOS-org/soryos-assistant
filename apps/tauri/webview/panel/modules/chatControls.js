export function createChatControls({ controller, metaButton, contextButton } = {}) {
  let metaVisible = false;
  let contextVisible = false;

  function updateMetaUi() {
    if (metaButton) {
      metaButton.textContent = metaVisible ? "Hide Details" : "Show Details";
      metaButton.ariaPressed = metaVisible ? "true" : "false";
    }
    if (controller?.setGlobalMetaVisible) {
      controller.setGlobalMetaVisible(metaVisible);
    }
  }

  function updateContextUi() {
    if (contextButton) {
      contextButton.textContent = contextVisible ? "Hide Context" : "Show Context";
      contextButton.ariaPressed = contextVisible ? "true" : "false";
    }
    if (controller?.setContextVisible) {
      controller.setContextVisible(contextVisible);
    }
  }

  function bind() {
    if (metaButton) {
      metaButton.addEventListener("click", () => {
        metaVisible = !metaVisible;
        updateMetaUi();
      });
    }

    if (contextButton) {
      contextButton.addEventListener("click", () => {
        contextVisible = !contextVisible;
        updateContextUi();
      });
    }
  }

  function refresh() {
    updateMetaUi();
    updateContextUi();
  }

  return {
    bind,
    refresh,
    setMetaVisible(value) {
      metaVisible = Boolean(value);
      updateMetaUi();
    },
    setContextVisible(value) {
      contextVisible = Boolean(value);
      updateContextUi();
    },
  };
}
