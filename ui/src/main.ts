import "../styles.css";
import {
  downloadModel,
  getAppVersion,
  getConfig,
  getDictationEnabled,
  listMicrophones,
  listModels,
  listenForDictationStatus,
  listenForModelDownloadProgress,
  openAccessibilitySettings,
  resizeSettingsWindow,
  saveAndApplyConfig,
  setDictationEnabled,
} from "./commands";
import type {
  Correction,
  DictationStatusEvent,
  HistoryEntry,
  HotkeyChoice,
  HotkeyOption,
  ModelChoice,
} from "./types";

const HOTKEY_CHOICES: readonly HotkeyOption[] = [
  { value: "Function", label: "🌐 Globe (fn)" },
  { value: "RightOption", label: "Right Option" },
  { value: "RightControl", label: "Right Control" },
  { value: "F8", label: "F8" },
  { value: "F9", label: "F9" },
];

const HISTORY_STORAGE_KEY = "rustle-history";
const HISTORY_LIMIT = 200;
const DEFAULT_MODEL_FILE_NAME = "ggml-base.en.bin";

function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (!element) {
    throw new Error(`missing #${id}`);
  }
  return element as T;
}

const elements = {
  statusPill: requiredElement<HTMLSpanElement>("status-pill"),
  dictationToggle: requiredElement<HTMLInputElement>("dictation-toggle"),
  dictationCaption: requiredElement<HTMLSpanElement>("dictation-caption"),
  hotkeySelect: requiredElement<HTMLSelectElement>("hotkey-select"),
  microphoneSelect: requiredElement<HTMLSelectElement>("microphone-select"),
  modelSelect: requiredElement<HTMLSelectElement>("model-select"),
  modelDownload: requiredElement<HTMLButtonElement>("model-download"),
  downloadStatus: requiredElement<HTMLParagraphElement>("download-status"),
  enterToggle: requiredElement<HTMLInputElement>("enter-toggle"),
  launchToggle: requiredElement<HTMLInputElement>("launch-toggle"),
  liveTranscript: requiredElement<HTMLTextAreaElement>("live-transcript"),
  historyList: requiredElement<HTMLDivElement>("history-list"),
  clearHistory: requiredElement<HTMLButtonElement>("clear-history"),
  correctionsList: requiredElement<HTMLDivElement>("corrections-list"),
  addCorrection: requiredElement<HTMLButtonElement>("add-correction"),
  appVersion: requiredElement<HTMLSpanElement>("app-version"),
  saveButton: requiredElement<HTMLButtonElement>("save-button"),
  saveNote: requiredElement<HTMLSpanElement>("save-note"),
  insertBanner: requiredElement<HTMLDivElement>("insert-banner"),
  insertNote: requiredElement<HTMLParagraphElement>("insert-note"),
  openAccessibility: requiredElement<HTMLButtonElement>("open-accessibility"),
};

let dictationHistory: HistoryEntry[] = [];
let selectedModelFileName = DEFAULT_MODEL_FILE_NAME;
let corrections: Correction[] = [];
let modelCatalog: ModelChoice[] = [];

function isHistoryEntry(value: unknown): value is HistoryEntry {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const entry = value as { text?: unknown; time?: unknown };
  return typeof entry.text === "string" && typeof entry.time === "string";
}

function loadHistory(): HistoryEntry[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) || "[]");
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter(isHistoryEntry).slice(0, HISTORY_LIMIT);
  } catch {
    return [];
  }
}

function saveHistory(): void {
  try {
    localStorage.setItem(
      HISTORY_STORAGE_KEY,
      JSON.stringify(dictationHistory.slice(0, HISTORY_LIMIT)),
    );
  } catch {
    return;
  }
}

function renderHistory(): void {
  elements.historyList.replaceChildren();
  for (const entry of dictationHistory) {
    const item = document.createElement("div");
    item.className = "history-item";
    const time = document.createElement("span");
    time.className = "history-time";
    time.textContent = entry.time;
    const text = document.createElement("span");
    text.textContent = entry.text;
    item.append(time, text);
    elements.historyList.appendChild(item);
  }
}

function addHistoryEntry(text: string | undefined): void {
  const trimmed = (text ?? "").trim();
  if (!trimmed) {
    return;
  }
  const stamp = new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  dictationHistory.unshift({ text: trimmed, time: stamp });
  dictationHistory = dictationHistory.slice(0, HISTORY_LIMIT);
  saveHistory();
  renderHistory();
  fitWindowToContent();
}

function fitWindowToContent(): void {
  requestAnimationFrame(() => {
    const app = document.querySelector(".app");
    const contentHeight =
      app instanceof HTMLElement ? app.offsetHeight : document.body.scrollHeight;
    void resizeSettingsWindow(contentHeight);
  });
}

function renderCorrections(): void {
  elements.correctionsList.replaceChildren();
  corrections.forEach((rule, index) => {
    const row = document.createElement("div");
    row.className = "correction-row";

    const spoken = document.createElement("input");
    spoken.type = "text";
    spoken.placeholder = "heard as…";
    spoken.value = rule.spoken;
    spoken.addEventListener("input", (event) => {
      const target = event.target;
      const current = corrections[index];
      if (target instanceof HTMLInputElement && current) {
        current.spoken = target.value;
      }
    });

    const arrow = document.createElement("span");
    arrow.className = "correction-arrow";
    arrow.textContent = "→";

    const written = document.createElement("input");
    written.type = "text";
    written.placeholder = "write as…";
    written.value = rule.written;
    written.addEventListener("input", (event) => {
      const target = event.target;
      const current = corrections[index];
      if (target instanceof HTMLInputElement && current) {
        current.written = target.value;
      }
    });

    const remove = document.createElement("button");
    remove.className = "correction-remove";
    remove.type = "button";
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      corrections.splice(index, 1);
      renderCorrections();
    });

    row.append(spoken, arrow, written, remove);
    elements.correctionsList.appendChild(row);
  });
  fitWindowToContent();
}

function isHotkeyChoice(value: string): value is HotkeyChoice {
  return HOTKEY_CHOICES.some((choice) => choice.value === value);
}

function populateHotkeys(selectedValue: HotkeyChoice): void {
  elements.hotkeySelect.replaceChildren();
  for (const choice of HOTKEY_CHOICES) {
    const option = document.createElement("option");
    option.value = choice.value;
    option.textContent = choice.label;
    option.selected = choice.value === selectedValue;
    elements.hotkeySelect.appendChild(option);
  }
}

async function populateMicrophones(selectedName: string | null): Promise<void> {
  const names = await listMicrophones().catch(() => [] as string[]);
  elements.microphoneSelect.replaceChildren();

  const defaultOption = document.createElement("option");
  defaultOption.value = "";
  defaultOption.textContent = "System default";
  elements.microphoneSelect.appendChild(defaultOption);

  for (const name of names) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    option.selected = name === selectedName;
    elements.microphoneSelect.appendChild(option);
  }
}

async function populateModels(): Promise<void> {
  modelCatalog = await listModels().catch(() => [] as ModelChoice[]);
  elements.modelSelect.replaceChildren();

  for (const model of modelCatalog) {
    const option = document.createElement("option");
    option.value = model.file_name;
    option.textContent = model.installed
      ? `${model.label} · installed`
      : `${model.label} · ${model.approximate_download} (not downloaded)`;
    option.selected = model.file_name === selectedModelFileName;
    elements.modelSelect.appendChild(option);
  }

  updateModelDownloadButton();
  fitWindowToContent();
}

function selectedModel(): ModelChoice | undefined {
  return modelCatalog.find((model) => model.file_name === elements.modelSelect.value);
}

function updateModelDownloadButton(): void {
  const model = selectedModel();
  elements.modelDownload.hidden = !(model && !model.installed);
}

async function downloadSelectedModel(): Promise<void> {
  const model = selectedModel();
  if (!model) {
    return;
  }
  elements.modelDownload.disabled = true;
  elements.modelDownload.textContent = "Downloading…";
  elements.downloadStatus.textContent = `Starting ${model.label}…`;
  try {
    await downloadModel(model.file_name, model.download_url);
    elements.downloadStatus.textContent = `${model.label} installed.`;
    await populateModels();
  } catch (error) {
    elements.downloadStatus.textContent = `Download failed: ${String(error)}`;
  } finally {
    elements.modelDownload.disabled = false;
    elements.modelDownload.textContent = "Download";
  }
}

function setStatus(text: string, className: string): void {
  elements.statusPill.textContent = text;
  elements.statusPill.className = `status-pill ${className}`;
}

function setInsertNote(text: string | undefined): void {
  const message = (text ?? "").trim();
  elements.insertNote.textContent = message;
  elements.insertBanner.hidden = message === "";
  fitWindowToContent();
}

function applyStatusEvent(payload: DictationStatusEvent): void {
  switch (payload.kind) {
    case "listening":
      setStatus("Listening", "status-live");
      break;
    case "partial":
      setStatus("Listening", "status-live");
      break;
    case "transcribing":
      setStatus("Transcribing", "status-work");
      break;
    case "typed":
      setStatus("Typed", "status-live");
      setInsertNote("");
      addHistoryEntry(payload.text);
      break;
    case "settings_preview":
      elements.liveTranscript.value = payload.text;
      break;
    case "needs_permission":
      setInsertNote(payload.text);
      break;
    case "failed":
      setStatus("Error", "status-fail");
      setInsertNote(payload.text);
      break;
    case "idle":
      setStatus("Idle", "status-idle");
      break;
  }
}

async function saveSettings(): Promise<void> {
  const microphoneValue = elements.microphoneSelect.value;
  const chosenModel = selectedModel();
  const installedFallback =
    modelCatalog.find((model) => model.installed)?.file_name ?? DEFAULT_MODEL_FILE_NAME;
  const modelFileName =
    chosenModel && chosenModel.installed ? chosenModel.file_name : installedFallback;
  if (chosenModel && !chosenModel.installed) {
    elements.saveNote.textContent = `${chosenModel.label} isn't downloaded yet, keeping ${modelFileName}`;
  }

  const selectedHotkey = elements.hotkeySelect.value;
  if (!isHotkeyChoice(selectedHotkey)) {
    elements.saveNote.textContent = "Could not save: unknown hotkey";
    return;
  }

  const config = {
    hotkey: selectedHotkey,
    model_file_name: modelFileName,
    input_device_name: microphoneValue === "" ? null : microphoneValue,
    launch_at_login: elements.launchToggle.checked,
    press_enter_on_release: elements.enterToggle.checked,
    corrections: corrections
      .map((rule) => ({ spoken: rule.spoken.trim(), written: rule.written.trim() }))
      .filter((rule) => rule.spoken !== ""),
  };

  elements.saveButton.disabled = true;
  try {
    await saveAndApplyConfig(config);
    elements.saveNote.textContent = "Saved.";
  } catch (error) {
    elements.saveNote.textContent = `Could not save: ${String(error)}`;
  } finally {
    elements.saveButton.disabled = false;
  }
}

function setUpTabs(): void {
  const tabs = document.querySelectorAll<HTMLButtonElement>(".tab");
  const panels = document.querySelectorAll<HTMLElement>(".tab-panel");
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      tabs.forEach((other) => other.classList.toggle("is-active", other === tab));
      const target = `panel-${tab.getAttribute("data-tab") ?? ""}`;
      panels.forEach((panel) => panel.classList.toggle("is-active", panel.id === target));
      fitWindowToContent();
    });
  });
}

async function initialise(): Promise<void> {
  const config = await getConfig();
  selectedModelFileName = config.model_file_name;
  try {
    elements.appVersion.textContent = await getAppVersion();
  } catch {
    elements.appVersion.textContent = "";
  }

  populateHotkeys(config.hotkey);
  await populateMicrophones(config.input_device_name);
  await populateModels();
  elements.enterToggle.checked = config.press_enter_on_release;
  elements.launchToggle.checked = config.launch_at_login;

  elements.modelSelect.addEventListener("change", () => {
    selectedModelFileName = elements.modelSelect.value;
    updateModelDownloadButton();
  });
  elements.modelDownload.addEventListener("click", () => {
    void downloadSelectedModel();
  });

  corrections = config.corrections.map((rule) => ({
    spoken: rule.spoken,
    written: rule.written,
  }));
  renderCorrections();
  elements.addCorrection.addEventListener("click", () => {
    corrections.push({ spoken: "", written: "" });
    renderCorrections();
  });

  const enabled = await getDictationEnabled().catch(() => true);
  elements.dictationToggle.checked = enabled;
  elements.dictationCaption.textContent = enabled
    ? "Listening for the hotkey"
    : "Paused";

  elements.dictationToggle.addEventListener("change", () => {
    void (async () => {
      const on = elements.dictationToggle.checked;
      await setDictationEnabled(on);
      elements.dictationCaption.textContent = on
        ? "Listening for the hotkey"
        : "Paused";
    })();
  });

  elements.saveButton.addEventListener("click", () => {
    void saveSettings();
  });
  elements.openAccessibility.addEventListener("click", () => {
    void openAccessibilitySettings();
  });

  await listenForDictationStatus(applyStatusEvent);
  await listenForModelDownloadProgress((progress) => {
    if (typeof progress.percent === "number") {
      elements.downloadStatus.textContent = `Downloading… ${progress.percent.toFixed(0)}%`;
    }
  });

  dictationHistory = loadHistory();
  renderHistory();
  elements.clearHistory.addEventListener("click", () => {
    dictationHistory = [];
    saveHistory();
    renderHistory();
    fitWindowToContent();
  });
  setUpTabs();
  fitWindowToContent();
}

void initialise();
