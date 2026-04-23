import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const ACTION_OPTIONS = [
  "enable_hiragana_key",
  "disable_hiragana_key",
  "forced_preedit_trigger_key",
  "kanchoku_bunsetsu_marker",
  "kanchoku_pure_trigger_key",
  "bunsetsu_prediction_cycle_key",
  "user_dictionary_editor_trigger",
  "force_commit_key",
  "to_katakana",
  "to_hiragana",
  "to_ascii",
  "to_zenkaku",
];

let currentState = null;

const elements = {
  status: document.querySelector("#status"),
  message: document.querySelector("#message"),
  layout: document.querySelector("#layout"),
  kanchokuLayout: document.querySelector("#kanchoku_layout"),
  candidateWindowSize: document.querySelector("#candidate_window_size"),
  keybindings: document.querySelector("#keybindings"),
  systemDictionaries: document.querySelector("#system-dictionaries"),
  userDictionaries: document.querySelector("#user-dictionaries"),
  extSystemDictionaries: document.querySelector("#ext-system-dictionaries"),
  extUserDictionaries: document.querySelector("#ext-user-dictionaries"),
  murensoList: document.querySelector("#murenso-list"),
  murensoFilterFirst: document.querySelector("#murenso-filter-first"),
  murensoFilterSecond: document.querySelector("#murenso-filter-second"),
  murensoFilterKanji: document.querySelector("#murenso-filter-kanji"),
  rawConfig: document.querySelector("#raw-config"),
  reload: document.querySelector("#reload"),
  save: document.querySelector("#save"),
  addKeybinding: document.querySelector("#add-keybinding"),
  convertSystemDicts: document.querySelector("#convert-system-dicts"),
  convertUserDicts: document.querySelector("#convert-user-dicts"),
  convertExtDicts: document.querySelector("#convert-ext-dicts"),
  reloadMurenso: document.querySelector("#reload-murenso"),
};

document.querySelectorAll(".nav-item").forEach((button) => {
  button.addEventListener("click", () => {
    document
      .querySelectorAll(".nav-item")
      .forEach((item) => item.classList.toggle("active", item === button));
    document.querySelectorAll(".panel").forEach((panel) => {
      panel.classList.toggle(
        "active",
        panel.id === `panel-${button.dataset.panel}`,
      );
    });
  });
});

// Nested sub-panel navigation for Dictionaries
document.querySelectorAll(".sub-nav-item").forEach((button) => {
  button.addEventListener("click", () => {
    document
      .querySelectorAll(".sub-nav-item")
      .forEach((item) => item.classList.toggle("active", item === button));
    document.querySelectorAll(".sub-panel").forEach((panel) => {
      panel.classList.toggle(
        "active",
        panel.id === `subpanel-${button.dataset.subpanel}`,
      );
    });
  });
});

const handleEscapeToClose = async (event) => {
  if (event.key !== "Escape") return;
  event.preventDefault();
  event.stopPropagation();
  await getCurrentWindow().close();
};

window.addEventListener("keydown", handleEscapeToClose, true);
document.addEventListener("keydown", handleEscapeToClose, true);

elements.reload.addEventListener("click", () => loadState());
elements.save.addEventListener("click", () => saveState());
elements.addKeybinding.addEventListener("click", () => {
  renderKeybindingRow("", "");
});
elements.convertSystemDicts.addEventListener("click", () => convertSystemDictionaries());
elements.convertUserDicts.addEventListener("click", () => convertUserDictionaries());
elements.convertExtDicts.addEventListener("click", () => convertExtendedDictionary());
elements.reloadMurenso.addEventListener("click", () => {
  if (!currentState) return;
  renderMurensoMappings(currentState.murenso_mappings || []);
});
elements.murensoFilterFirst.addEventListener("input", (e) => {
  if (e.target.value.length > 1) {
    e.target.value = e.target.value.slice(-1);
  }
  applyMurensoFilter();
});
elements.murensoFilterSecond.addEventListener("input", (e) => {
  if (e.target.value.length > 1) {
    e.target.value = e.target.value.slice(-1);
  }
  applyMurensoFilter();
});
elements.murensoFilterKanji.addEventListener("input", () => applyMurensoFilter());
elements.kanchokuLayout.addEventListener("change", async () => {
  setStatus("Loading murenso mappings for new layout…");
  try {
    const result = await invoke("load_kanchoku_mappings", {
      layoutId: elements.kanchokuLayout.value,
    });
    renderMurensoMappings(result);
    setStatus("Murenso mappings updated");
  } catch (error) {
    console.error(error);
    setStatus("Failed to load murenso mappings");
  }
});

loadState();

async function loadState() {
  setStatus("Loading settings state…");
  try {
    currentState = await invoke("load_settings_state");
    renderState(currentState);
    setStatus("Settings state loaded");
    setMessage(currentState.warnings?.join("\n") || "");
  } catch (error) {
    console.error(error);
    setStatus("Failed to load settings state");
    setMessage(String(error));
  }
}

async function saveState() {
  if (!currentState) return;

  setStatus("Saving settings…");
  try {
    const result = await invoke("save_settings_state", {
      input: collectSaveInput(),
      murensoMappings: collectMurensoMappings(),
    });
    currentState = result.state;
    renderState(currentState);

    if (!result.saved) {
      setStatus("Save blocked by keybinding conflicts");
      setMessage(
        Object.entries(result.keybinding_conflicts)
          .map(([key, actions]) => `${key}: ${actions.join(", ")}`)
          .join("\n"),
      );
      return;
    }

    setStatus("Settings saved");
    setMessage(result.config_path ? `Saved to ${result.config_path}` : "Saved");
  } catch (error) {
    console.error(error);
    setStatus("Failed to save settings");
    setMessage(String(error));
  }
}

async function convertSystemDictionaries() {
  setStatus("Converting system dictionaries…");
  try {
    const result = await invoke("convert_system_dictionaries", {
      sourceWeights: collectDictionaryWeights("system"),
    });
    currentState = result.state;
    renderState(currentState);
    setStatus("System dictionary conversion complete");
    setMessage(formatDictionaryResult(result));
  } catch (error) {
    console.error(error);
    setStatus("System dictionary conversion failed");
    setMessage(String(error));
  }
}

async function convertUserDictionaries() {
  setStatus("Converting user dictionaries…");
  try {
    const result = await invoke("convert_user_dictionaries", {
      sourceWeights: collectDictionaryWeights("user"),
    });
    currentState = result.state;
    renderState(currentState);
    setStatus("User dictionary conversion complete");
    setMessage(formatDictionaryResult(result));
  } catch (error) {
    console.error(error);
    setStatus("User dictionary conversion failed");
    setMessage(String(error));
  }
}

async function convertExtendedDictionary() {
  setStatus("Generating extended dictionary…");
  try {
    const result = await invoke("convert_extended_dictionary", {
      sourcePaths: collectExtSourcePaths(),
    });
    currentState = result.state;
    renderState(currentState);
    setStatus("Extended dictionary generation complete");
    setMessage(formatExtendedDictionaryResult(result));
  } catch (error) {
    console.error(error);
    setStatus("Extended dictionary generation failed");
    setMessage(String(error));
  }
}

function renderState(state) {
  renderSelect(elements.layout, state.layouts, state.config.layout);
  renderSelect(
    elements.kanchokuLayout,
    state.kanchoku_layouts,
    state.config.kanchoku_layout,
  );

  const ui = state.config.ui || {};
  elements.candidateWindowSize.value = ui.candidate_window_size ?? 9;

  renderKeybindingsFromConfig(state.config);
  renderSystemDictionaries(state.system_dictionaries);
  renderUserDictionaries(state.user_dictionaries);
  renderExtDictionaries(elements.extSystemDictionaries, state.ext_system_dictionaries);
  renderExtDictionaries(elements.extUserDictionaries, state.ext_user_dictionaries);
  renderMurensoMappings(state.murenso_mappings);
  elements.rawConfig.textContent = JSON.stringify(state.config, null, 2);
}

function renderSelect(element, items, selectedId) {
  element.innerHTML = "";
  for (const item of items) {
    const option = document.createElement("option");
    option.value = item.id;
    option.textContent = item.display_label;
    option.selected = item.id === selectedId;
    element.appendChild(option);
  }
}

function renderKeybindingsFromConfig(config) {
  elements.keybindings.innerHTML = "";
  const topLevel = [
    "enable_hiragana_key",
    "disable_hiragana_key",
    "forced_preedit_trigger_key",
    "kanchoku_bunsetsu_marker",
    "kanchoku_pure_trigger_key",
    "bunsetsu_prediction_cycle_key",
    "user_dictionary_editor_trigger",
    "force_commit_key",
  ];

  for (const action of topLevel) {
    for (const key of config[action] || []) {
      renderKeybindingRow(action, key);
    }
  }

  const conversionKeys = config.conversion_keys || {};
  for (const action of ["to_katakana", "to_hiragana", "to_ascii", "to_zenkaku"]) {
    for (const key of conversionKeys[action] || []) {
      renderKeybindingRow(action, key);
    }
  }
}

function renderKeybindingRow(actionId, keyValue) {
  const row = document.createElement("div");
  row.className = "keybinding-row";

  const actionSelect = document.createElement("select");
  for (const action of ACTION_OPTIONS) {
    const option = document.createElement("option");
    option.value = action;
    option.textContent = action;
    option.selected = action === actionId;
    actionSelect.appendChild(option);
  }

  const keyInput = document.createElement("input");
  keyInput.type = "text";
  keyInput.value = keyValue;
  keyInput.placeholder = "Control+Shift+K";
  keyInput.readOnly = true;

  const captureButton = document.createElement("button");
  captureButton.textContent = "Capture";
  captureButton.className = "ghost";
  captureButton.addEventListener("click", () => openKeyCapture(keyInput));

  const removeButton = document.createElement("button");
  removeButton.textContent = "Remove";
  removeButton.className = "ghost";
  removeButton.addEventListener("click", () => row.remove());

  row.appendChild(actionSelect);
  row.appendChild(keyInput);
  row.appendChild(captureButton);
  row.appendChild(removeButton);
  elements.keybindings.appendChild(row);
}

function renderSystemDictionaries(entries) {
  elements.systemDictionaries.innerHTML = "";
  for (const entry of entries) {
    const row = document.createElement("div");
    row.className = "dict-table-row";
    row.dataset.path = entry.full_path;  // Use full_path for backend matching
    
    // Enabled checkbox
    const enabledCell = document.createElement("div");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = entry.enabled;
    checkbox.dataset.type = "system";
    enabledCell.appendChild(checkbox);
    
    // Dictionary name (read-only)
    const nameCell = document.createElement("div");
    nameCell.className = "dict-name";
    nameCell.textContent = entry.relative_path;  // Display relative path
    nameCell.title = entry.full_path;
    
    // Weight input
    const weightCell = document.createElement("div");
    const weightInput = document.createElement("input");
    weightInput.type = "number";
    weightInput.value = entry.weight;
    weightInput.min = "0";
    weightInput.step = "1";
    weightInput.dataset.type = "system";
    weightCell.appendChild(weightInput);
    
    row.appendChild(enabledCell);
    row.appendChild(nameCell);
    row.appendChild(weightCell);
    elements.systemDictionaries.appendChild(row);
  }
}

function renderUserDictionaries(entries) {
  elements.userDictionaries.innerHTML = "";
  for (const entry of entries) {
    const row = document.createElement("div");
    row.className = "dict-table-row";
    row.dataset.path = entry.filename;  // Use filename as the key for user dicts
    
    // Enabled checkbox
    const enabledCell = document.createElement("div");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = entry.enabled;
    checkbox.dataset.type = "user";
    enabledCell.appendChild(checkbox);
    
    // Dictionary name (read-only)
    const nameCell = document.createElement("div");
    nameCell.className = "dict-name";
    nameCell.textContent = entry.filename;
    nameCell.title = entry.filename;
    
    // Weight input
    const weightCell = document.createElement("div");
    const weightInput = document.createElement("input");
    weightInput.type = "number";
    weightInput.value = entry.weight;
    weightInput.min = "0";
    weightInput.step = "1";
    weightInput.dataset.type = "user";
    weightCell.appendChild(weightInput);
    
    row.appendChild(enabledCell);
    row.appendChild(nameCell);
    row.appendChild(weightCell);
    elements.userDictionaries.appendChild(row);
  }
}

function renderExtDictionaries(container, entries) {
  container.innerHTML = "";
  for (const entry of entries) {
    const row = document.createElement("div");
    row.className = "dict-table-row two-col";
    row.dataset.path = entry.full_path;

    // Include checkbox
    const checkboxCell = document.createElement("div");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = entry.enabled;
    checkboxCell.appendChild(checkbox);

    // Dictionary name (read-only)
    const nameCell = document.createElement("div");
    nameCell.className = "dict-name";
    nameCell.textContent = entry.display_name;
    nameCell.title = entry.full_path;

    row.appendChild(checkboxCell);
    row.appendChild(nameCell);
    container.appendChild(row);
  }
}

function renderDictionaryEntry({ label, fullPath, enabled, weight, datasetType }) {
  const row = document.createElement("label");
  row.className = "dictionary-row";

  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = enabled;
  checkbox.dataset.path = fullPath;
  checkbox.dataset.type = datasetType;

  const text = document.createElement("span");
  text.textContent = label;

  const weightInput = document.createElement("input");
  weightInput.type = "number";
  weightInput.min = "1";
  weightInput.step = "1";
  weightInput.value = String(weight || 1);
  weightInput.dataset.path = fullPath;
  weightInput.dataset.type = datasetType;

  row.appendChild(checkbox);
  row.appendChild(text);
  row.appendChild(weightInput);
  return row;
}

function collectSaveInput() {
  const keybindingsByAction = {};
  for (const row of elements.keybindings.querySelectorAll(".keybinding-row")) {
    const action = row.querySelector("select").value;
    const key = row.querySelector("input[type=text]").value.trim();
    if (!action || !key) continue;
    keybindingsByAction[action] ||= [];
    keybindingsByAction[action].push(key);
  }

  return {
    layout: elements.layout.value,
    kanchoku_layout: elements.kanchokuLayout.value,
    candidate_window_size: Number(elements.candidateWindowSize.value || 9),
    keybindings_by_action: keybindingsByAction,
    enabled_system_dicts: collectDictionaryWeights("system"),
    enabled_user_dicts: collectDictionaryWeights("user"),
  };
}

function collectDictionaryWeights(type) {
  const result = {};
  const selector =
    type === "system" ? "#system-dictionaries .dict-table-row" : "#user-dictionaries .dict-table-row";

  for (const row of document.querySelectorAll(selector)) {
    const checkbox = row.querySelector("input[type=checkbox]");
    const weight = row.querySelector("input[type=number]");
    if (checkbox && checkbox.checked) {
      const path = row.dataset.path;
      if (path) {
        result[path] = Math.max(1, Number(weight.value || 1));
      }
    }
  }
  return result;
}

function collectMurensoMappings() {
  const result = [];
  for (const row of elements.murensoList.querySelectorAll(".murenso-row")) {
    const firstKey = row.querySelector("[data-field=first_key]").value.trim();
    const secondKey = row.querySelector("[data-field=second_key]").value.trim();
    const kanji = row.querySelector("[data-field=kanji]").value.trim();
    if (!firstKey && !secondKey && !kanji) continue;
    result.push({
      first_key: firstKey,
      second_key: secondKey,
      kanji,
    });
  }
  return result;
}

function collectExtSourcePaths() {
  const result = [];
  for (const selector of ["#ext-system-dictionaries", "#ext-user-dictionaries"]) {
    for (const row of document.querySelectorAll(`${selector} .dict-table-row`)) {
      const checkbox = row.querySelector("input[type=checkbox]");
      if (checkbox && checkbox.checked) {
        const path = row.dataset.path;
        if (path) {
          result.push(path);
        }
      }
    }
  }
  return result;
}

function renderMurensoMappings(mappings) {
  elements.murensoList.innerHTML = "";
  for (const mapping of mappings) {
    elements.murensoList.appendChild(renderMurensoRow(mapping));
  }
  if (!mappings.length) {
    elements.murensoList.appendChild(
      renderMurensoRow({ first_key: "", second_key: "", kanji: "" }),
    );
  }
  applyMurensoFilter();
}

function renderMurensoRow(mapping) {
  const row = document.createElement("div");
  row.className = "murenso-row";

  row.appendChild(createMurensoInput("first_key", mapping.first_key || "", 1, true));
  row.appendChild(createMurensoInput("second_key", mapping.second_key || "", 1, true));
  row.appendChild(createMurensoInput("kanji", mapping.kanji || "", 8, true));

  return row;
}

function createMurensoInput(field, value, maxLength, readOnly = false) {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.dataset.field = field;
  if (maxLength) input.maxLength = maxLength;
  if (readOnly) {
    input.readOnly = true;
    input.style.cursor = "default";
    input.style.backgroundColor = "var(--bg)";
  }
  if (field !== "kanji") {
    input.placeholder = "1 key";
  } else {
    input.placeholder = "漢字";
  }
  if (!readOnly) {
    input.addEventListener("input", () => {
      if (field !== "kanji" && input.value.length > 1) {
        input.value = input.value.slice(0, 1);
      }
      applyMurensoFilter();
    });
  }
  return input;
}

function applyMurensoFilter() {
  const filterFirst = elements.murensoFilterFirst.value.trim().toLowerCase();
  const filterSecond = elements.murensoFilterSecond.value.trim().toLowerCase();
  const filterKanji = elements.murensoFilterKanji.value.trim().toLowerCase();
  
  for (const row of elements.murensoList.querySelectorAll(".murenso-row")) {
    const inputs = row.querySelectorAll("input");
    const firstKey = inputs[0]?.value.toLowerCase() || "";
    const secondKey = inputs[1]?.value.toLowerCase() || "";
    const kanji = inputs[2]?.value.toLowerCase() || "";
    
    const matchesFirst = !filterFirst || firstKey.includes(filterFirst);
    const matchesSecond = !filterSecond || secondKey.includes(filterSecond);
    const matchesKanji = !filterKanji || kanji.includes(filterKanji);
    
    row.classList.toggle("hidden", !(matchesFirst && matchesSecond && matchesKanji));
  }
}

function stripColorPrefix(value) {
  return String(value).replace(/^0x/i, "").replace(/^#/, "");
}

function formatDictionaryResult(result) {
  const lines = [];
  if (result.output_path) lines.push(`Output: ${result.output_path}`);
  lines.push(`Files processed: ${result.files_processed}`);
  lines.push(`Total readings: ${result.total_readings}`);
  lines.push(`Total candidates: ${result.total_candidates}`);
  if (result.okurigana_entries_expanded) {
    lines.push(`Okurigana expanded: ${result.okurigana_entries_expanded}`);
  }
  return lines.join("\n");
}

function formatExtendedDictionaryResult(result) {
  const lines = [];
  if (result.output_path) lines.push(`Output: ${result.output_path}`);
  lines.push(`Files processed: ${result.files_processed}`);
  lines.push(`Kanchoku kanji: ${result.kanchoku_kanji_count}`);
  lines.push(`Yomi→kanji mappings: ${result.yomi_kanji_mappings}`);
  lines.push(`Dictionary entries scanned: ${result.source_entries_scanned}`);
  lines.push(`Total readings: ${result.total_readings}`);
  lines.push(`Total candidates: ${result.total_candidates}`);
  return lines.join("\n");
}

function setStatus(value) {
  elements.status.textContent = value;
}

function setMessage(value) {
  elements.message.textContent = value;
}

// Key Capture Modal
let currentKeyInput = null;
let capturedKeyValue = null;

const modal = document.getElementById("key-capture-modal");
const modalClose = document.getElementById("modal-close");
const modalCancel = document.getElementById("modal-cancel");
const modalAccept = document.getElementById("modal-accept");
const capturedKeySpan = document.getElementById("captured-key");
const capturedCodeSpan = document.getElementById("captured-code");
const capturedModifiersSpan = document.getElementById("captured-modifiers");

function openKeyCapture(inputElement) {
  currentKeyInput = inputElement;
  capturedKeyValue = null;
  capturedKeySpan.textContent = "—";
  capturedCodeSpan.textContent = "—";
  capturedModifiersSpan.textContent = "—";
  modal.style.display = "flex";
  modalAccept.disabled = true;
}

function closeKeyCapture() {
  modal.style.display = "none";
  currentKeyInput = null;
  capturedKeyValue = null;
}

function handleKeyCapture(event) {
  if (modal.style.display !== "flex") return;
  
  event.preventDefault();
  event.stopPropagation();
  
  // Build modifier string
  const modifiers = [];
  if (event.ctrlKey) modifiers.push("Control");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  if (event.metaKey) modifiers.push("Meta");
  
  // Use event.key for the key representation
  let key = event.key;
  
  // Normalize special keys to readable names
  const keyNormalizations = {
    " ": "Space",
    "Enter": "Enter",
    "Tab": "Tab",
    "Escape": "Escape",
    "Backspace": "Backspace",
    "Delete": "Delete",
    "ArrowUp": "ArrowUp",
    "ArrowDown": "ArrowDown",
    "ArrowLeft": "ArrowLeft",
    "ArrowRight": "ArrowRight",
  };
  
  if (keyNormalizations[key]) {
    key = keyNormalizations[key];
  }
  
  // Build the full key string (e.g., "Control+Shift+K")
  const keyParts = [...modifiers];
  if (key && key !== "Control" && key !== "Alt" && key !== "Shift" && key !== "Meta") {
    keyParts.push(key);
  }
  
  capturedKeyValue = keyParts.join("+");
  
  // Display captured information (show normalized key name)
  capturedKeySpan.textContent = key || "—";
  capturedCodeSpan.textContent = event.code || "—";
  capturedModifiersSpan.textContent = modifiers.length > 0 ? modifiers.join(", ") : "None";
  
  modalAccept.disabled = keyParts.length === 0;
}

modalClose.addEventListener("click", closeKeyCapture);
modalCancel.addEventListener("click", closeKeyCapture);
modalAccept.addEventListener("click", () => {
  if (currentKeyInput && capturedKeyValue) {
    currentKeyInput.value = capturedKeyValue;
  }
  closeKeyCapture();
});

// Close modal when clicking outside
modal.addEventListener("click", (e) => {
  if (e.target === modal) {
    closeKeyCapture();
  }
});

// Capture keyboard events globally when modal is open
document.addEventListener("keydown", handleKeyCapture);
