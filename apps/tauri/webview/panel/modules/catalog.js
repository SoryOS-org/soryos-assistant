import { MODEL_FIELD_NAME } from "./constants.js";
import { configState } from "./state.js";

export function setModelCatalog(entries = []) {
  configState.modelCatalog = Array.isArray(entries) ? entries : [];
}

export function getModelCatalog() {
  return configState.modelCatalog;
}

function findCatalogEntryById(id) {
  const catalog = getModelCatalog();
  if (!catalog.length || !id) {
    return null;
  }
  return catalog.find((item) => item.id === id) || null;
}

export function friendlyLabel(id) {
  const entry = findCatalogEntryById(id);
  if (entry) {
    return entry.label;
  }
  return id || "-";
}

export function populateModelSelect(select) {
  if (!select) {
    return;
  }

  const catalog = getModelCatalog();
  select.innerHTML = "";

  catalog.forEach((entry) => {
    const option = document.createElement("option");
    option.value = entry.id;
    option.textContent = entry.label;
    select.appendChild(option);
  });
}

export function ensureModelOption(select, id) {
  if (!select || !id) {
    return;
  }

  const entry = findCatalogEntryById(id);
  if (entry) {
    return;
  }
  const exists = Array.from(select.options).some((option) => option.value === id);

  if (!exists) {
    const option = document.createElement("option");
    option.value = id;
    option.textContent = `${friendlyLabel(id)} (custom)`;
    select.appendChild(option);
  }
}

export function currentModelIdFromForm(form) {
  return form?.elements?.[MODEL_FIELD_NAME]?.value ?? "";
}

function setSelectValue(select, id) {
  if (!select) {
    return;
  }

  if (!id) {
    select.value = "";
    return;
  }

  select.value = id;

  // If the option isn't present yet, add it and reassign.
  if (select.value !== id) {
    ensureModelOption(select, id);
    select.value = id;
  }
}

export function applyModelSelection(select, id) {
  if (!select) {
    return;
  }
  const entry = findCatalogEntryById(id);
  if (entry) {
    setSelectValue(select, entry.id);
    return;
  }
  ensureModelOption(select, id);
  setSelectValue(select, id);
}

export function syncModelSelectValue(select, id) {
  if (!select) {
    return;
  }

  populateModelSelect(select);
  applyModelSelection(select, id);
}
