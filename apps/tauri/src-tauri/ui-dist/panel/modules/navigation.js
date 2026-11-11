export function createNavigation({ navSelector, viewSelector, onNavigate } = {}) {
  const navButtons = new Map();
  document.querySelectorAll(navSelector || ".panel-nav__item").forEach((button) => {
    const viewId = button?.dataset?.view;
    if (viewId) {
      navButtons.set(viewId, button);
    }
  });

  const viewRegistry = new Map();
  document.querySelectorAll(viewSelector || ".panel-view").forEach((section) => {
    const viewId = section?.dataset?.view;
    if (viewId) {
      viewRegistry.set(viewId, { id: viewId, element: section, onEnter: null });
    }
  });

  let currentView =
    document.querySelector(".panel-view.panel-view--active")?.dataset.view || "chat";

  function setNavSelection(viewId) {
    navButtons.forEach((button, id) => {
      button.classList.toggle("is-active", id === viewId);
    });
  }

  function setViewVisibility(viewId) {
    viewRegistry.forEach((entry, id) => {
      if (!entry.element) return;
      entry.element.classList.toggle("panel-view--active", id === viewId);
    });
  }

  function activate(viewId, { force = false } = {}) {
    const entry = viewRegistry.get(viewId);
    if (!entry) {
      return;
    }

    const isSame = currentView === viewId;
    setNavSelection(viewId);
    setViewVisibility(viewId);

    if (!isSame) {
      currentView = viewId;
    }

    if ((!isSame || force) && entry.onEnter) {
      entry.onEnter();
    }
  }

  function requestNavigation(viewId) {
    if (!viewRegistry.has(viewId)) {
      return;
    }
    activate(viewId);
    if (typeof onNavigate === "function") {
      onNavigate(viewId);
    }
  }

  function registerOnEnter(viewId, handler) {
    const entry = viewRegistry.get(viewId);
    if (entry) {
      entry.onEnter = handler;
    }
  }

  function current() {
    return currentView;
  }

  return {
    activate,
    requestNavigation,
    registerOnEnter,
    current,
  };
}
