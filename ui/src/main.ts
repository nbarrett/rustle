import "../styles.css";
import {
  checkForAppUpdate,
  downloadAndInstallAppUpdate,
  downloadModel,
  getAppVersion,
  getConfig,
  getDictationEnabled,
  hostPlatform,
  installIntoApplications,
  listHotkeyChoices,
  listMicrophones,
  listModels,
  listenForDictationStatus,
  listenForMacosSetup,
  listenForModelDownloadProgress,
  macosSetupStatus,
  notifyHotkeyEdge,
  openPermissionSettings,
  requestDictationPermissions,
  readUtf8Path,
  relaunchApp,
  resizeSettingsWindow,
  saveAndApplyConfig,
  setDictationEnabled,
  writeUtf8Path,
} from "./commands";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Update } from "@tauri-apps/plugin-updater";
import type {
  Config,
  Correction,
  DictationStatusEvent,
  HistoryEntry,
  HotkeyChoice,
  HotkeyOption,
  MacosSetupStatus,
  ModelChoice,
  PermissionPane,
} from "./types";

let hotkeyChoices: HotkeyOption[] = [
  { value: "Function", label: "fn (Globe)" },
  { value: "RightOption", label: "Right Option" },
  { value: "RightControl", label: "Right Control" },
  { value: "F8", label: "F8" },
  { value: "F9", label: "F9" },
];

function hostOsFromUserAgent(): string {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Linux/i.test(ua) && !/Android/i.test(ua)) return "linux";
  return "macos";
}

let platformName = hostOsFromUserAgent();
document.documentElement.dataset.os = platformName;

const HISTORY_STORAGE_KEY = "rustle-history";
const LAST_TRANSCRIPT_STORAGE_KEY = "rustle-last-transcript";
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
  hotkeyHint: requiredElement<HTMLParagraphElement>("hotkey-hint"),
  microphoneSelect: requiredElement<HTMLSelectElement>("microphone-select"),
  modelSelect: requiredElement<HTMLSelectElement>("model-select"),
  modelDownload: requiredElement<HTMLButtonElement>("model-download"),
  downloadStatus: requiredElement<HTMLParagraphElement>("download-status"),
  enterToggle: requiredElement<HTMLInputElement>("enter-toggle"),
  enterCaption: requiredElement<HTMLSpanElement>("enter-caption"),
  launchToggle: requiredElement<HTMLInputElement>("launch-toggle"),
  liveTranscript: requiredElement<HTMLTextAreaElement>("live-transcript"),
  historyList: requiredElement<HTMLDivElement>("history-list"),
  clearHistory: requiredElement<HTMLButtonElement>("clear-history"),
  correctionsList: requiredElement<HTMLDivElement>("corrections-list"),
  correctionsSearch: requiredElement<HTMLInputElement>("corrections-search"),
  addCorrection: requiredElement<HTMLButtonElement>("add-correction"),
  exportCorrections: requiredElement<HTMLButtonElement>("export-corrections"),
  importCorrections: requiredElement<HTMLButtonElement>("import-corrections"),
  correctionsFileNote: requiredElement<HTMLParagraphElement>("corrections-file-note"),
  exportHistory: requiredElement<HTMLButtonElement>("export-history"),
  importHistory: requiredElement<HTMLButtonElement>("import-history"),
  historyFileNote: requiredElement<HTMLParagraphElement>("history-file-note"),
  appVersion: requiredElement<HTMLSpanElement>("app-version"),
  saveButton: requiredElement<HTMLButtonElement>("save-button"),
  saveNote: requiredElement<HTMLSpanElement>("save-note"),
  insertBanner: requiredElement<HTMLDivElement>("insert-banner"),
  insertNote: requiredElement<HTMLParagraphElement>("insert-note"),
  openAccessibility: requiredElement<HTMLButtonElement>("open-accessibility"),
  setupBanner: requiredElement<HTMLDivElement>("setup-banner"),
  setupNote: requiredElement<HTMLParagraphElement>("setup-note"),
  setupList: requiredElement<HTMLUListElement>("setup-list"),
  setupPermissions: requiredElement<HTMLButtonElement>("setup-permissions"),
  setupInstall: requiredElement<HTMLButtonElement>("setup-install"),
  updateBanner: requiredElement<HTMLDivElement>("update-banner"),
  updateNote: requiredElement<HTMLParagraphElement>("update-note"),
  installUpdate: requiredElement<HTMLButtonElement>("install-update"),
  wordReplace: requiredElement<HTMLDivElement>("word-replace"),
  wordReplaceFrom: requiredElement<HTMLSpanElement>("word-replace-from"),
  wordReplaceTo: requiredElement<HTMLInputElement>("word-replace-to"),
  wordReplaceCancel: requiredElement<HTMLButtonElement>("word-replace-cancel"),
  wordReplaceSave: requiredElement<HTMLButtonElement>("word-replace-save"),
};

let dictationHistory: HistoryEntry[] = [];
let selectedModelFileName = DEFAULT_MODEL_FILE_NAME;
let corrections: Correction[] = [];
let modelCatalog: ModelChoice[] = [];
let wordReplaceEntryIndex: number | null = null;
let wordReplaceSpoken = "";
let pendingUpdate: Update | null = null;
let insertBannerPane: PermissionPane = "accessibility";

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
  dictationHistory.forEach((entry, entryIndex) => {
    const item = document.createElement("div");
    item.className = "history-item";
    const time = document.createElement("span");
    time.className = "history-time";
    time.textContent = entry.time;
    const text = document.createElement("span");
    text.className = "history-text";
    appendHistoryWords(text, entry.text, entryIndex);
    item.append(time, text);
    elements.historyList.appendChild(item);
  });
}

function appendHistoryWords(
  container: HTMLElement,
  text: string,
  entryIndex: number,
): void {
  const parts = text.split(/([A-Za-z0-9]+(?:['’][A-Za-z0-9]+)*)/);
  for (const part of parts) {
    if (part === "") {
      continue;
    }
    if (/^[A-Za-z0-9]/.test(part)) {
      const word = document.createElement("span");
      word.className = "history-word";
      word.textContent = part;
      word.addEventListener("dblclick", (event) => {
        event.preventDefault();
        event.stopPropagation();
        openWordReplacement(entryIndex, part);
      });
      container.append(word);
    } else {
      container.append(part);
    }
  }
}

function openWordReplacement(entryIndex: number, spoken: string): void {
  wordReplaceEntryIndex = entryIndex;
  wordReplaceSpoken = spoken;
  elements.wordReplaceFrom.textContent = spoken;
  const existing = corrections.find(
    (rule) => rule.spoken.toLocaleLowerCase() === spoken.toLocaleLowerCase(),
  );
  elements.wordReplaceTo.value = existing?.written ?? "";
  elements.wordReplace.hidden = false;
  elements.wordReplaceTo.focus();
  elements.wordReplaceTo.select();
}

function closeWordReplacement(): void {
  wordReplaceEntryIndex = null;
  wordReplaceSpoken = "";
  elements.wordReplace.hidden = true;
  elements.wordReplaceTo.value = "";
}

function replaceWordInText(text: string, spoken: string, written: string): string {
  const escaped = spoken.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return text.replace(new RegExp(escaped, "gi"), written);
}

async function persistCorrections(): Promise<void> {
  const config = await getConfig();
  await saveAndApplyConfig(
    {
      ...config,
      corrections: corrections
        .map((rule) => ({ spoken: rule.spoken.trim(), written: rule.written.trim() }))
        .filter((rule) => rule.spoken !== ""),
    },
    false,
  );
}

async function saveWordReplacement(): Promise<void> {
  const written = elements.wordReplaceTo.value.trim();
  const spoken = wordReplaceSpoken.trim();
  const entryIndex = wordReplaceEntryIndex;
  if (spoken === "" || written === "" || entryIndex === null) {
    return;
  }
  const existing = corrections.find(
    (rule) => rule.spoken.toLocaleLowerCase() === spoken.toLocaleLowerCase(),
  );
  if (existing) {
    existing.written = written;
  } else {
    corrections.push({ spoken, written });
  }
  const entry = dictationHistory[entryIndex];
  if (entry) {
    entry.text = replaceWordInText(entry.text, spoken, written);
    saveHistory();
  }
  renderCorrections();
  renderHistory();
  closeWordReplacement();
  try {
    await persistCorrections();
  } catch {
    return;
  }
  fitWindowToContent();
}

function persistLastTranscript(text: string): void {
  try {
    localStorage.setItem(LAST_TRANSCRIPT_STORAGE_KEY, text);
  } catch {
    return;
  }
}

function showLastTranscript(text: string, persist: boolean): void {
  elements.liveTranscript.value = text;
  elements.liveTranscript.scrollTop = elements.liveTranscript.scrollHeight;
  if (persist) {
    persistLastTranscript(text);
  }
}

function loadLastTranscript(): string {
  try {
    return localStorage.getItem(LAST_TRANSCRIPT_STORAGE_KEY) ?? "";
  } catch {
    return "";
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
  showLastTranscript(trimmed, true);
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

function trimmedCorrections(): Correction[] {
  return corrections
    .map((rule) => ({ spoken: rule.spoken.trim(), written: rule.written.trim() }))
    .filter((rule) => rule.spoken !== "");
}

function isCorrectionRecord(value: unknown): value is Correction {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as { spoken?: unknown; written?: unknown };
  return typeof record.spoken === "string" && typeof record.written === "string";
}

function correctionsFromUnknown(value: unknown): Correction[] {
  if (Array.isArray(value)) {
    return value.filter(isCorrectionRecord).map((rule) => ({
      spoken: rule.spoken.trim(),
      written: rule.written.trim(),
    }));
  }
  if (typeof value === "object" && value !== null) {
    const record = value as { corrections?: unknown };
    if (Array.isArray(record.corrections)) {
      return correctionsFromUnknown(record.corrections);
    }
    return [];
  }
  throw new Error("file is not a list of word corrections");
}

function historyFromUnknown(value: unknown): HistoryEntry[] {
  if (typeof value !== "object" || value === null) {
    return [];
  }
  const record = value as { history?: unknown };
  if (!Array.isArray(record.history)) {
    return [];
  }
  return record.history.filter(isHistoryEntry).map((entry) => ({
    text: entry.text.trim(),
    time: entry.time,
  }));
}

function mergeImportedCorrections(incoming: Correction[]): number {
  let added = 0;
  for (const rule of incoming) {
    if (rule.spoken === "") {
      continue;
    }
    const existing = corrections.find(
      (current) => current.spoken.toLocaleLowerCase() === rule.spoken.toLocaleLowerCase(),
    );
    if (existing) {
      existing.written = rule.written;
    } else {
      corrections.push({ spoken: rule.spoken, written: rule.written });
      added += 1;
    }
  }
  return added;
}

function mergeImportedHistory(incoming: HistoryEntry[]): number {
  const seen = new Set(
    dictationHistory.map((entry) => `${entry.time}\0${entry.text}`),
  );
  const extra: HistoryEntry[] = [];
  for (const entry of incoming) {
    if (entry.text === "") {
      continue;
    }
    const key = `${entry.time}\0${entry.text}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    extra.push({ text: entry.text, time: entry.time });
  }
  dictationHistory = [...dictationHistory, ...extra].slice(0, HISTORY_LIMIT);
  saveHistory();
  return extra.length;
}

function setWordsFileNote(text: string): void {
  elements.correctionsFileNote.textContent = text;
  elements.historyFileNote.textContent = text;
}

function importedWordsSummary(correctionsAdded: number, historyAdded: number): string {
  const correctionBit =
    correctionsAdded === 1 ? "1 correction" : `${correctionsAdded} corrections`;
  const historyBit =
    historyAdded === 1 ? "1 history entry" : `${historyAdded} history entries`;
  return `Imported ${correctionBit} and ${historyBit}.`;
}

async function exportWordsToFile(): Promise<void> {
  const path = await save({
    defaultPath: "rustle-words.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) {
    return;
  }
  const body = JSON.stringify(
    {
      kind: "rustle-words",
      version: 1,
      corrections: trimmedCorrections(),
      history: dictationHistory.slice(0, HISTORY_LIMIT),
    },
    null,
    2,
  );
  await writeUtf8Path(path, `${body}\n`);
  setWordsFileNote("Exported corrections and history.");
}

async function importWordsFromFile(): Promise<void> {
  const chosen = await open({
    multiple: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!chosen || Array.isArray(chosen)) {
    return;
  }
  const parsed: unknown = JSON.parse(await readUtf8Path(chosen));
  const incomingCorrections = correctionsFromUnknown(parsed);
  const incomingHistory = historyFromUnknown(parsed);
  if (incomingCorrections.length === 0 && incomingHistory.length === 0) {
    throw new Error("file has no corrections or history");
  }
  const correctionsAdded = mergeImportedCorrections(incomingCorrections);
  const historyAdded = mergeImportedHistory(incomingHistory);
  renderCorrections();
  renderHistory();
  await persistCorrections();
  setWordsFileNote(importedWordsSummary(correctionsAdded, historyAdded));
}

function correctionMatchesSearch(rule: Correction, query: string): boolean {
  if (query === "") {
    return true;
  }
  const spoken = rule.spoken.toLocaleLowerCase();
  const written = rule.written.toLocaleLowerCase();
  return spoken.includes(query) || written.includes(query);
}

function visibleCorrections(): Correction[] {
  const query = elements.correctionsSearch.value.trim().toLocaleLowerCase();
  return corrections.filter((rule) => correctionMatchesSearch(rule, query));
}

function renderCorrections(): void {
  const query = elements.correctionsSearch.value.trim();
  const visible = visibleCorrections();
  elements.correctionsList.replaceChildren();
  if (visible.length === 0) {
    const empty = document.createElement("p");
    empty.className = "field-hint corrections-empty";
    empty.textContent = query === "" ? "No corrections yet." : "No corrections match.";
    elements.correctionsList.appendChild(empty);
    fitWindowToContent();
    return;
  }
  for (const rule of visible) {
    const row = document.createElement("div");
    row.className = "correction-row";

    const spoken = document.createElement("input");
    spoken.type = "text";
    spoken.placeholder = "heard as…";
    spoken.value = rule.spoken;
    spoken.addEventListener("input", (event) => {
      const target = event.target;
      if (target instanceof HTMLInputElement) {
        rule.spoken = target.value;
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
      if (target instanceof HTMLInputElement) {
        rule.written = target.value;
      }
    });

    const remove = document.createElement("button");
    remove.className = "correction-remove";
    remove.type = "button";
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      const index = corrections.indexOf(rule);
      if (index >= 0) {
        corrections.splice(index, 1);
      }
      renderCorrections();
    });

    row.append(spoken, arrow, written, remove);
    elements.correctionsList.appendChild(row);
  }
  fitWindowToContent();
}

function isHotkeyChoice(value: string): value is HotkeyChoice {
  return hotkeyChoices.some((choice) => choice.value === value);
}

function populateHotkeys(selectedValue: HotkeyChoice): void {
  const selected = isHotkeyChoice(selectedValue)
    ? selectedValue
    : (hotkeyChoices[0]?.value ?? "RightControl");
  elements.hotkeySelect.replaceChildren();
  for (const choice of hotkeyChoices) {
    const option = document.createElement("option");
    option.value = choice.value;
    option.textContent = choice.label;
    option.selected = choice.value === selected;
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

function statusSnippet(text: string | undefined): string {
  const trimmed = (text ?? "").trim();
  if (trimmed === "") {
    return "Listening";
  }
  if (trimmed.length <= 36) {
    return trimmed;
  }
  return `${trimmed.slice(0, 36)}…`;
}

function insertBannerAction(message: string): {
  label: string;
  pane: PermissionPane;
} {
  const lower = message.toLowerCase();
  if (
    lower.includes("microphone") ||
    lower.includes("recording") ||
    lower.includes("audio") ||
    lower.includes("coreaudio")
  ) {
    return { label: "Open Microphone settings", pane: "microphone" };
  }
  if (lower.includes("input monitoring")) {
    return { label: "Open Input Monitoring settings", pane: "listen" };
  }
  if (platformName === "windows") {
    return { label: "Open microphone settings", pane: "microphone" };
  }
  return { label: "Open Accessibility settings", pane: "accessibility" };
}

function setInsertNote(text: string | undefined): void {
  const message = (text ?? "").trim();
  elements.insertNote.textContent = message;
  elements.insertBanner.hidden = message === "";
  if (message !== "") {
    const action = insertBannerAction(message);
    elements.openAccessibility.textContent = action.label;
    insertBannerPane = action.pane;
  }
  fitWindowToContent();
}

function applyStatusEvent(payload: DictationStatusEvent): void {
  switch (payload.kind) {
    case "listening":
      setStatus("Listening", "status-live");
      break;
    case "partial":
      setStatus(statusSnippet(payload.text), "status-live");
      showLastTranscript(payload.text ?? "", false);
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
      showLastTranscript(payload.text ?? "", false);
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

async function saveSettings(hideWindow = true): Promise<void> {
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
    await saveAndApplyConfig(config, hideWindow);
    elements.saveNote.textContent = hideWindow
      ? "Saved."
      : `Using ${elements.hotkeySelect.selectedOptions[0]?.textContent ?? "that key"}.`;
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

function renderSetupStatus(status: MacosSetupStatus): void {
  if (platformName !== "macos") {
    elements.setupBanner.hidden = true;
    return;
  }
  const microphone = status.microphone !== false;
  const ready =
    status.in_applications && status.listen && status.accessibility && microphone;
  elements.setupBanner.hidden = ready;
  elements.setupInstall.hidden = status.in_applications;
  const rows: Array<[boolean, string]> = [
    [status.in_applications, "Installed in Applications"],
    [status.listen, "Input Monitoring: hears the hotkey in other apps"],
    [status.accessibility, "Accessibility: types into the app you are using"],
    [microphone, "Microphone: records what you say"],
  ];
  elements.setupList.replaceChildren();
  for (const [ok, label] of rows) {
    const item = document.createElement("li");
    item.textContent = `${ok ? "On" : "Needs setup"}: ${label}`;
    elements.setupList.appendChild(item);
  }
  if (ready) {
    elements.setupNote.textContent = "Setup is complete.";
  } else if (status.listen && status.accessibility && !microphone) {
    elements.setupNote.textContent =
      "Allow the microphone, then hold the push-to-talk key.";
  } else if (status.listen && status.accessibility) {
    elements.setupNote.textContent = "Permissions are on. Restarting…";
  } else {
    elements.setupNote.textContent =
      "Turn on the switches macOS shows, then wait. Rustle will restart itself.";
  }
  fitWindowToContent();
}

async function refreshSetupStatus(): Promise<void> {
  if (platformName !== "macos") {
    elements.setupBanner.hidden = true;
    return;
  }
  try {
    renderSetupStatus(await macosSetupStatus());
  } catch {
    elements.setupBanner.hidden = true;
  }
}

function eventMatchesPushToTalk(event: KeyboardEvent, hotkey: string): boolean {
  switch (hotkey) {
    case "F8":
      return event.code === "F8" || event.key === "F8" || event.key === "MediaPlayPause";
    case "F9":
      return event.code === "F9" || event.key === "F9" || event.key === "MediaTrackNext";
    case "RightControl":
      return event.code === "ControlRight";
    case "RightOption":
      return event.code === "AltRight";
    default:
      return false;
  }
}

function listenForPushToTalkInThisWindow(): void {
  if (platformName !== "windows") {
    return;
  }
  window.addEventListener(
    "keydown",
    (event) => {
      if (event.repeat) {
        return;
      }
      if (!eventMatchesPushToTalk(event, elements.hotkeySelect.value)) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      void notifyHotkeyEdge(true);
    },
    true,
  );
  window.addEventListener(
    "keyup",
    (event) => {
      if (!eventMatchesPushToTalk(event, elements.hotkeySelect.value)) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      void notifyHotkeyEdge(false);
    },
    true,
  );
}

function applyPlatformCopy(): void {
  document.documentElement.dataset.os = platformName;
  elements.enterCaption.textContent =
    platformName === "macos"
      ? "Press Return when you release the key"
      : "Press Enter when you release the key";
  elements.openAccessibility.textContent =
    platformName === "macos"
      ? "Open Accessibility settings"
      : platformName === "windows"
        ? "Open microphone settings"
        : "Open system settings";
  elements.hotkeyHint.textContent =
    platformName === "windows"
      ? "Hold to talk, release to type. On a Mac keyboard use Fn with F8 or F9. Parallels captures Right Control as its host key, so pick Right Alt or F9 there."
      : "Hold to talk, release to type into the focused app.";
}

async function checkForAvailableUpdate(): Promise<void> {
  try {
    pendingUpdate = await checkForAppUpdate();
  } catch {
    pendingUpdate = null;
  }
  if (!pendingUpdate) {
    elements.updateBanner.hidden = true;
    fitWindowToContent();
    return;
  }
  elements.updateNote.textContent = `Version ${pendingUpdate.version} is available.`;
  elements.installUpdate.hidden = false;
  elements.installUpdate.disabled = false;
  elements.installUpdate.textContent = "Install update";
  elements.updateBanner.hidden = false;
  fitWindowToContent();
}

async function installAvailableUpdate(): Promise<void> {
  if (!pendingUpdate) {
    return;
  }
  elements.installUpdate.disabled = true;
  elements.installUpdate.textContent = "Installing…";
  elements.updateNote.textContent = `Downloading ${pendingUpdate.version}…`;
  try {
    await downloadAndInstallAppUpdate(pendingUpdate);
    elements.updateNote.textContent = "Restarting…";
    await relaunchApp();
  } catch (error) {
    elements.updateNote.textContent = `Update failed: ${String(error)}`;
    elements.installUpdate.disabled = false;
    elements.installUpdate.textContent = "Install update";
  }
}

function fallbackConfig(): Config {
  return {
    hotkey: hotkeyChoices[0]?.value ?? "RightControl",
    model_file_name: DEFAULT_MODEL_FILE_NAME,
    input_device_name: null,
    launch_at_login: false,
    press_enter_on_release: false,
    corrections: [],
  };
}

async function initialise(): Promise<void> {
  platformName = await hostPlatform().catch(hostOsFromUserAgent);
  const listedHotkeys = await listHotkeyChoices().catch(() => [] as HotkeyOption[]);
  if (listedHotkeys.length > 0) {
    hotkeyChoices = listedHotkeys;
  }
  applyPlatformCopy();
  listenForPushToTalkInThisWindow();
  await refreshSetupStatus();
  await listenForMacosSetup(renderSetupStatus);
  elements.setupPermissions.addEventListener("click", () => {
    void requestDictationPermissions();
  });
  elements.setupInstall.addEventListener("click", () => {
    void installIntoApplications().catch((error) => {
      elements.setupNote.textContent = `Could not install: ${String(error)}`;
    });
  });
  if (platformName === "macos") {
    window.setInterval(() => {
      void refreshSetupStatus();
    }, 1500);
  }
  populateHotkeys(hotkeyChoices[0]?.value ?? "RightControl");
  const config = await getConfig().catch(fallbackConfig);
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
  elements.correctionsSearch.addEventListener("input", () => {
    renderCorrections();
  });
  elements.addCorrection.addEventListener("click", () => {
    elements.correctionsSearch.value = "";
    corrections.push({ spoken: "", written: "" });
    renderCorrections();
    const spokenField = elements.correctionsList.querySelector(
      ".correction-row:last-child input",
    );
    if (spokenField instanceof HTMLInputElement) {
      spokenField.focus();
      spokenField.scrollIntoView({ block: "nearest" });
    }
  });
  const exportWords = () => {
    void exportWordsToFile().catch((error) => {
      setWordsFileNote(`Could not export: ${String(error)}`);
    });
  };
  const importWords = () => {
    void importWordsFromFile().catch((error) => {
      setWordsFileNote(`Could not import: ${String(error)}`);
    });
  };
  elements.exportCorrections.addEventListener("click", exportWords);
  elements.importCorrections.addEventListener("click", importWords);
  elements.exportHistory.addEventListener("click", exportWords);
  elements.importHistory.addEventListener("click", importWords);

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

  elements.hotkeySelect.addEventListener("change", () => {
    void saveSettings(false);
  });
  elements.saveButton.addEventListener("click", () => {
    void saveSettings();
  });
  elements.openAccessibility.addEventListener("click", () => {
    void openPermissionSettings(insertBannerPane);
  });
  elements.installUpdate.addEventListener("click", () => {
    void installAvailableUpdate();
  });
  void checkForAvailableUpdate();

  await listenForDictationStatus(applyStatusEvent);
  await listenForModelDownloadProgress((progress) => {
    if (typeof progress.percent === "number") {
      elements.downloadStatus.textContent = `Downloading… ${progress.percent.toFixed(0)}%`;
    }
  });

  dictationHistory = loadHistory();
  renderHistory();
  const savedTranscript = loadLastTranscript();
  if (savedTranscript !== "") {
    showLastTranscript(savedTranscript, false);
  } else if (dictationHistory[0]) {
    showLastTranscript(dictationHistory[0].text, true);
  }
  elements.liveTranscript.addEventListener("input", () => {
    persistLastTranscript(elements.liveTranscript.value);
  });
  elements.liveTranscript.addEventListener("focus", () => {
    elements.liveTranscript.scrollTop = elements.liveTranscript.scrollHeight;
  });
  elements.clearHistory.addEventListener("click", () => {
    dictationHistory = [];
    saveHistory();
    renderHistory();
    fitWindowToContent();
  });
  elements.wordReplaceCancel.addEventListener("click", () => {
    closeWordReplacement();
  });
  elements.wordReplaceSave.addEventListener("click", () => {
    void saveWordReplacement();
  });
  elements.wordReplaceTo.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void saveWordReplacement();
    }
    if (event.key === "Escape") {
      event.preventDefault();
      closeWordReplacement();
    }
  });
  elements.wordReplace.addEventListener("click", (event) => {
    if (event.target === elements.wordReplace) {
      closeWordReplacement();
    }
  });
  setUpTabs();
  fitWindowToContent();
}

void initialise().catch((error) => {
  populateHotkeys(hotkeyChoices[0]?.value ?? "RightControl");
  void populateMicrophones(null);
  void populateModels();
  elements.saveNote.textContent = `Could not load settings: ${String(error)}`;
});
