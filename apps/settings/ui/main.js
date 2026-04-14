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
  showAnnotations: document.querySelector("#show_annotations"),
  candidateWindowSize: document.querySelector("#candidate_window_size"),
  preeditForeground: document.querySelector("#preedit_foreground_color"),
  preeditBackground: document.querySelector("#preedit_background_color"),
  useIbusHint: document.querySelector("#use_ibus_hint_colors"),
  keybindings: document.querySelector("#keybindings"),
  systemDictionaries: document.querySelector("#system-dictionaries"),
  userDictionaries: document.querySelector("#user-dictionaries"),
  extSystemDictionaries: document.querySelector("#ext-system-dictionaries"),
  extUserDictionaries: document.querySelector("#ext-user-dictionaries"),
  murensoList: document.querySelector("#murenso-list"),
  murensoFilter: document.querySelector("#murenso-filter"),
  rawConfig: document.querySelector("#raw-config"),
  reload: document.querySelector("#reload"),
  save: document.querySelector("#save"),
  addKeybinding: document.querySelector("#add-keybinding"),
  convertSystemDicts: document.querySelector("#convert-system-dicts"),
  convertUserDicts: document.querySelector("#convert-user-dicts"),
  convertExtDicts: document.querySelector("#convert-ext-dicts"),
  addMurenso: document.querySelector("#add-murenso"),
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
elements.addMurenso.addEventListener("click", () => {
  elements.murensoList.appendChild(renderMurensoRow({ first_key: "", second_key: "", kanji: "" }));
  applyMurensoFilter();
});
elements.reloadMurenso.addEventListener("click", () => {
  if (!currentState) return;
  renderMurensoMappings(currentState.murenso_mappings || []);
});
elements.murensoFilter.addEventListener("input", () => applyMurensoFilter());

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
  elements.showAnnotations.checked = Boolean(ui.show_annotations);
  elements.candidateWindowSize.value = ui.candidate_window_size ?? 9;
  elements.preeditForeground.value = stripColorPrefix(
    state.config.preedit_foreground_color || "0x000000",
  );
  elements.preeditBackground.value = stripColorPrefix(
    state.config.preedit_background_color || "0xd1eaff",
  );
  elements.useIbusHint.checked = Boolean(state.config.use_ibus_hint_colors);

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

  const removeButton = document.createElement("button");
  removeButton.textContent = "Remove";
  removeButton.className = "ghost";
  removeButton.addEventListener("click", () => row.remove());

  row.appendChild(actionSelect);
  row.appendChild(keyInput);
  row.appendChild(removeButton);
  elements.keybindings.appendChild(row);
}

function renderSystemDictionaries(entries) {
  elements.systemDictionaries.innerHTML = "";
  for (const entry of entries) {
    elements.systemDictionaries.appendChild(renderDictionaryEntry({
      label: entry.relative_path,
      fullPath: entry.full_path,
      enabled: entry.enabled,
      weight: entry.weight,
      datasetType: "system",
    }));
  }
}

function renderUserDictionaries(entries) {
  elements.userDictionaries.innerHTML = "";
  for (const entry of entries) {
    elements.userDictionaries.appendChild(renderDictionaryEntry({
      label: entry.filename,
      fullPath: entry.filename,
      enabled: entry.enabled,
      weight: entry.weight,
      datasetType: "user",
    }));
  }
}

function renderExtDictionaries(container, entries) {
  container.innerHTML = "";
  for (const entry of entries) {
    const row = document.createElement("label");
    row.className = "dictionary-row compact";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = entry.enabled;
    checkbox.dataset.path = entry.full_path;

    const text = document.createElement("span");
    text.textContent = entry.display_name;

    row.appendChild(checkbox);
    row.appendChild(text);
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
    show_annotations: elements.showAnnotations.checked,
    candidate_window_size: Number(elements.candidateWindowSize.value || 9),
    preedit_foreground_color: elements.preeditForeground.value,
    preedit_background_color: elements.preeditBackground.value,
    use_ibus_hint_colors: elements.useIbusHint.checked,
    keybindings_by_action: keybindingsByAction,
    enabled_system_dicts: collectDictionaryWeights("system"),
    enabled_user_dicts: collectDictionaryWeights("user"),
  };
}

function collectDictionaryWeights(type) {
  const result = {};
  const selector =
    type === "system" ? "#system-dictionaries .dictionary-row" : "#user-dictionaries .dictionary-row";

  for (const row of document.querySelectorAll(selector)) {
    const checkbox = row.querySelector("input[type=checkbox]");
    const weight = row.querySelector("input[type=number]");
    if (checkbox.checked) {
      result[checkbox.dataset.path] = Math.max(1, Number(weight.value || 1));
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
    for (const checkbox of document.querySelectorAll(`${selector} input[type=checkbox]`)) {
      if (checkbox.checked) {
        result.push(checkbox.dataset.path);
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

  row.appendChild(createMurensoInput("first_key", mapping.first_key || "", 1));
  row.appendChild(createMurensoInput("second_key", mapping.second_key || "", 1));
  row.appendChild(createMurensoInput("kanji", mapping.kanji || "", 8));

  const removeButton = document.createElement("button");
  removeButton.textContent = "Remove";
  removeButton.className = "ghost";
  removeButton.addEventListener("click", () => {
    row.remove();
    if (!elements.murensoList.children.length) {
      elements.murensoList.appendChild(
        renderMurensoRow({ first_key: "", second_key: "", kanji: "" }),
      );
    }
  });
  row.appendChild(removeButton);
  return row;
}

function createMurensoInput(field, value, maxLength) {
  const input = document.createElement("input");
  input.type = "text";
  input.value = value;
  input.dataset.field = field;
  if (maxLength) input.maxLength = maxLength;
  if (field !== "kanji") {
    input.placeholder = "1 key";
  } else {
    input.placeholder = "漢字";
  }
  input.addEventListener("input", () => {
    if (field !== "kanji" && input.value.length > 1) {
      input.value = input.value.slice(0, 1);
    }
    applyMurensoFilter();
  });
  return input;
}

function applyMurensoFilter() {
  const filter = elements.murensoFilter.value.trim().toLowerCase();
  for (const row of elements.murensoList.querySelectorAll(".murenso-row")) {
    const text = Array.from(row.querySelectorAll("input"))
      .map((input) => input.value)
      .join(" ")
      .toLowerCase();
    row.classList.toggle("hidden", Boolean(filter) && !text.includes(filter));
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
