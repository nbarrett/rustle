import init, {
  polish_transcript_for_the_clipboard,
  starting_corrections_as_json,
} from "../wasm/pkg/rustle_web_core";

type Correction = { spoken: string; written: string };
type HistoryEntry = { text: string; time: string };

type WorkerReply =
  | { kind: "ready"; modelId: string }
  | { kind: "progress"; percent: number; file: string }
  | { kind: "notice"; message: string }
  | { kind: "transcribed"; text: string }
  | { kind: "failed"; message: string };

const MODEL_CHOICES = [
  { id: "Xenova/whisper-tiny.en", label: "Tiny English, fastest, roughly 40 MB" },
  { id: "Xenova/whisper-base.en", label: "Base English, balanced, roughly 80 MB" },
  { id: "Xenova/whisper-small.en", label: "Small English, most accurate, roughly 250 MB" },
];
const DEFAULT_MODEL_ID = "Xenova/whisper-base.en";
const CORRECTIONS_STORAGE_KEY = "rustle-web-corrections";
const BRITISH_STORAGE_KEY = "rustle-web-prefers-british";
const MODEL_STORAGE_KEY = "rustle-web-model";
const HISTORY_STORAGE_KEY = "rustle-web-history";
const HISTORY_LIMIT = 200;
const WHISPER_SAMPLE_RATE = 16000;
const SHORTEST_USEFUL_CLIP_SECONDS = 0.25;

function requiredElement<T extends HTMLElement>(id: string): T {
  const found = document.getElementById(id);
  if (!found) {
    throw new Error(`the page is missing #${id}`);
  }
  return found as T;
}

const elements = {
  statusPill: requiredElement<HTMLSpanElement>("status-pill"),
  engineStatus: requiredElement<HTMLParagraphElement>("engine-status"),
  modelChoice: requiredElement<HTMLSelectElement>("model-choice"),
  loadModel: requiredElement<HTMLButtonElement>("load-model"),
  loadProgress: requiredElement<HTMLProgressElement>("load-progress"),
  holdToTalk: requiredElement<HTMLButtonElement>("hold-to-talk"),
  dictationStatus: requiredElement<HTMLParagraphElement>("dictation-status"),
  transcript: requiredElement<HTMLTextAreaElement>("transcript"),
  copyTranscript: requiredElement<HTMLButtonElement>("copy-transcript"),
  clearTranscript: requiredElement<HTMLButtonElement>("clear-transcript"),
  clipboardStatus: requiredElement<HTMLSpanElement>("clipboard-status"),
  correctionsList: requiredElement<HTMLDivElement>("corrections-list"),
  correctionsSearch: requiredElement<HTMLInputElement>("corrections-search"),
  addCorrection: requiredElement<HTMLButtonElement>("add-correction"),
  exportCorrections: requiredElement<HTMLButtonElement>("export-corrections"),
  importCorrections: requiredElement<HTMLButtonElement>("import-corrections"),
  correctionsFile: requiredElement<HTMLInputElement>("corrections-file"),
  prefersBritish: requiredElement<HTMLInputElement>("prefers-british"),
  correctionsStatus: requiredElement<HTMLParagraphElement>("corrections-status"),
  historyList: requiredElement<HTMLDivElement>("history-list"),
  exportHistory: requiredElement<HTMLButtonElement>("export-history"),
  importHistory: requiredElement<HTMLButtonElement>("import-history"),
  clearHistory: requiredElement<HTMLButtonElement>("clear-history"),
  historyFileNote: requiredElement<HTMLParagraphElement>("history-file-note"),
  wordReplace: requiredElement<HTMLDivElement>("word-replace"),
  wordReplaceFrom: requiredElement<HTMLSpanElement>("word-replace-from"),
  wordReplaceTo: requiredElement<HTMLInputElement>("word-replace-to"),
  wordReplaceCancel: requiredElement<HTMLButtonElement>("word-replace-cancel"),
  wordReplaceSave: requiredElement<HTMLButtonElement>("word-replace-save"),
  saveNote: requiredElement<HTMLSpanElement>("save-note"),
  saveSettings: requiredElement<HTMLButtonElement>("save-settings"),
};

const transcriberWorker = new Worker(new URL("./transcriber-worker.ts", import.meta.url), {
  type: "module",
});

let modelIsReady = false;
let recordingStream: MediaStream | null = null;
let clipRecorder: MediaRecorder | null = null;
let recordedChunks: Blob[] = [];
let currentlyRecording = false;
let transcriptionInFlight = false;
let corrections: Correction[] = [];
let dictationHistory: HistoryEntry[] = [];
let wordReplaceEntryIndex: number | null = null;
let wordReplaceSpoken = "";

function showStatus(element: HTMLElement, message: string, isProblem = false): void {
  element.textContent = message;
  element.classList.toggle("is-problem", isProblem);
}

function showEngineState(label: string, state: "idle" | "live" | "work" | "fail"): void {
  elements.statusPill.textContent = label;
  elements.statusPill.className = `status-pill status-${state}`;
}

function trimmedCorrections(): Correction[] {
  return corrections
    .map((rule) => ({ spoken: rule.spoken.trim(), written: rule.written.trim() }))
    .filter((rule) => rule.spoken !== "");
}

function valueIsCorrection(value: unknown): value is Correction {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { spoken?: unknown; written?: unknown };
  return typeof candidate.spoken === "string" && typeof candidate.written === "string";
}

function correctionsFromImportedValue(value: unknown): Correction[] {
  if (Array.isArray(value)) {
    return value.filter(valueIsCorrection).map((rule) => ({
      spoken: rule.spoken.trim(),
      written: rule.written.trim(),
    }));
  }
  if (typeof value === "object" && value !== null) {
    const candidate = value as { corrections?: unknown };
    if (Array.isArray(candidate.corrections)) {
      return correctionsFromImportedValue(candidate.corrections);
    }
  }
  return [];
}

function historyFromImportedValue(value: unknown): HistoryEntry[] {
  if (typeof value !== "object" || value === null) {
    return [];
  }
  const candidate = value as { history?: unknown };
  if (!Array.isArray(candidate.history)) {
    return [];
  }
  return candidate.history.filter(valueIsHistoryEntry).map((entry) => ({
    text: entry.text.trim(),
    time: entry.time,
  }));
}

function mergeCorrections(current: Correction[], incoming: Correction[]): Correction[] {
  const merged = current.map((rule) => ({ ...rule }));
  for (const rule of incoming) {
    if (rule.spoken === "") {
      continue;
    }
    const existing = merged.find(
      (candidate) => candidate.spoken.toLocaleLowerCase() === rule.spoken.toLocaleLowerCase(),
    );
    if (existing) {
      existing.written = rule.written;
    } else {
      merged.push({ ...rule });
    }
  }
  return merged;
}

function mergeHistory(incoming: HistoryEntry[]): number {
  const seen = new Set(dictationHistory.map((entry) => `${entry.time}\0${entry.text}`));
  let added = 0;
  for (const entry of incoming) {
    const key = `${entry.time}\0${entry.text}`;
    if (entry.text === "" || seen.has(key)) {
      continue;
    }
    seen.add(key);
    dictationHistory.push({ ...entry });
    added += 1;
  }
  dictationHistory = dictationHistory.slice(0, HISTORY_LIMIT);
  return added;
}

async function importWordsFromFile(file: File): Promise<void> {
  try {
    const parsed: unknown = JSON.parse(await file.text());
    const incoming = correctionsFromImportedValue(parsed);
    const incomingHistory = historyFromImportedValue(parsed);
    if (incoming.length === 0 && incomingHistory.length === 0) {
      throw new Error("The file contains no corrections or history.");
    }
    corrections = mergeCorrections(trimmedCorrections(), incoming);
    const historyAdded = mergeHistory(incomingHistory);
    renderCorrections();
    renderHistory();
    localStorage.setItem(CORRECTIONS_STORAGE_KEY, JSON.stringify(corrections));
    saveHistory();
    const correctionNoun = incoming.length === 1 ? "correction" : "corrections";
    const historyNoun = historyAdded === 1 ? "history entry" : "history entries";
    const result = `Imported ${incoming.length} ${correctionNoun} and ${historyAdded} ${historyNoun} from ${file.name}.`;
    showStatus(elements.correctionsStatus, result);
    showStatus(elements.historyFileNote, result);
  } catch (error) {
    showStatus(
      elements.correctionsStatus,
      error instanceof Error ? error.message : "The corrections file could not be read.",
      true,
    );
  } finally {
    elements.correctionsFile.value = "";
  }
}

function storedCorrections(): Correction[] {
  try {
    const saved = localStorage.getItem(CORRECTIONS_STORAGE_KEY);
    if (saved) {
      return JSON.parse(saved) as Correction[];
    }
  } catch {
    showStatus(elements.correctionsStatus, "Saved corrections could not be read, using defaults.", true);
  }
  return JSON.parse(starting_corrections_as_json()) as Correction[];
}

function activeCorrectionsAsJson(): string {
  return JSON.stringify(trimmedCorrections());
}

function valueIsHistoryEntry(value: unknown): value is HistoryEntry {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { text?: unknown; time?: unknown };
  return typeof candidate.text === "string" && typeof candidate.time === "string";
}

function loadHistory(): HistoryEntry[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(HISTORY_STORAGE_KEY) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter(valueIsHistoryEntry).slice(0, HISTORY_LIMIT) : [];
  } catch {
    return [];
  }
}

function saveHistory(): void {
  try {
    localStorage.setItem(HISTORY_STORAGE_KEY, JSON.stringify(dictationHistory));
  } catch {
    showStatus(elements.clipboardStatus, "History could not be saved in this browser.", true);
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
    text.addEventListener("mouseup", () => {
      const spoken = selectedPhraseInside(text);
      if (spoken?.includes(" ")) {
        openWordReplacement(entryIndex, spoken);
      }
    });
    item.append(time, text);
    elements.historyList.appendChild(item);
  });
}

function appendHistoryWords(container: HTMLElement, text: string, entryIndex: number): void {
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
      container.appendChild(word);
    } else {
      container.append(part);
    }
  }
}

function selectedPhraseInside(container: HTMLElement): string | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
    return null;
  }
  const range = selection.getRangeAt(0);
  if (!container.contains(range.commonAncestorContainer)) {
    return null;
  }
  const spoken = selection.toString().replace(/\s+/g, " ").trim();
  return spoken === "" ? null : spoken;
}

function openWordReplacement(entryIndex: number, spoken: string): void {
  window.getSelection()?.removeAllRanges();
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

function isCorrectionWordCharacter(character: string): boolean {
  return /[A-Za-z0-9]/.test(character);
}

function replacePhraseInText(text: string, spoken: string, written: string): string {
  const lowerText = text.toLocaleLowerCase();
  const lowerSpoken = spoken.toLocaleLowerCase();
  let result = "";
  let index = 0;
  while (index < text.length) {
    const found = lowerText.indexOf(lowerSpoken, index);
    if (found === -1) {
      return result + text.slice(index);
    }
    const end = found + spoken.length;
    const preceded = found > 0 && isCorrectionWordCharacter(text.charAt(found - 1));
    const followed = end < text.length && isCorrectionWordCharacter(text.charAt(end));
    result += text.slice(index, found);
    result += preceded || followed ? text.slice(found, end) : written;
    index = end;
  }
  return result;
}

function saveWordReplacement(): void {
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
    entry.text = replacePhraseInText(entry.text, spoken, written);
  }
  localStorage.setItem(CORRECTIONS_STORAGE_KEY, JSON.stringify(trimmedCorrections()));
  saveHistory();
  renderCorrections();
  renderHistory();
  closeWordReplacement();
}

function exportWordsToFile(): void {
  const body = JSON.stringify(
    {
      kind: "rustle-words",
      version: 1,
      corrections: trimmedCorrections(),
      history: dictationHistory,
    },
    null,
    2,
  );
  const url = URL.createObjectURL(new Blob([`${body}\n`], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = "rustle-words.json";
  link.click();
  URL.revokeObjectURL(url);
}

function rememberTranscript(text: string): void {
  const time = new Date().toLocaleString([], {
    dateStyle: "medium",
    timeStyle: "short",
  });
  dictationHistory.unshift({ text, time });
  dictationHistory = dictationHistory.slice(0, HISTORY_LIMIT);
  saveHistory();
  renderHistory();
}

function correctionMatchesSearch(rule: Correction, query: string): boolean {
  if (query === "") {
    return true;
  }
  return (
    rule.spoken.toLocaleLowerCase().includes(query) ||
    rule.written.toLocaleLowerCase().includes(query)
  );
}

function renderCorrections(): void {
  const query = elements.correctionsSearch.value.trim().toLocaleLowerCase();
  const visible = corrections.filter((rule) => correctionMatchesSearch(rule, query));
  elements.correctionsList.replaceChildren();
  if (visible.length === 0) {
    const empty = document.createElement("p");
    empty.className = "field-hint corrections-empty";
    empty.textContent = query === "" ? "No corrections yet." : "No corrections match.";
    elements.correctionsList.appendChild(empty);
    return;
  }
  for (const rule of visible) {
    const row = document.createElement("div");
    row.className = "correction-row";
    const spoken = document.createElement("input");
    spoken.type = "text";
    spoken.placeholder = "heard as…";
    spoken.value = rule.spoken;
    spoken.addEventListener("input", () => {
      rule.spoken = spoken.value;
    });
    const arrow = document.createElement("span");
    arrow.className = "correction-arrow";
    arrow.textContent = "→";
    const written = document.createElement("input");
    written.type = "text";
    written.placeholder = "write as…";
    written.value = rule.written;
    written.addEventListener("input", () => {
      rule.written = written.value;
    });
    const remove = document.createElement("button");
    remove.className = "correction-remove";
    remove.type = "button";
    remove.textContent = "✕";
    remove.addEventListener("click", () => {
      corrections = corrections.filter((candidate) => candidate !== rule);
      renderCorrections();
    });
    row.append(spoken, arrow, written, remove);
    elements.correctionsList.appendChild(row);
  }
}

function selectTab(name: string): void {
  document.querySelectorAll<HTMLElement>(".tab").forEach((tab) => {
    tab.classList.toggle("is-active", tab.dataset.tab === name);
  });
  document.querySelectorAll<HTMLElement>(".tab-panel").forEach((panel) => {
    panel.classList.toggle("is-active", panel.id === `panel-${name}`);
  });
}

function fillModelChoices(): void {
  for (const choice of MODEL_CHOICES) {
    const option = document.createElement("option");
    option.value = choice.id;
    option.textContent = choice.label;
    elements.modelChoice.append(option);
  }
  elements.modelChoice.value = localStorage.getItem(MODEL_STORAGE_KEY) ?? DEFAULT_MODEL_ID;
}

function loadSelectedModel(): void {
  modelIsReady = false;
  elements.holdToTalk.disabled = true;
  elements.loadModel.disabled = true;
  showEngineState("Loading", "work");
  showStatus(elements.engineStatus, "Loading the model, first time only…");
  showStatus(elements.dictationStatus, "Preparing speech recognition…");
  transcriberWorker.postMessage({ kind: "load", modelId: elements.modelChoice.value });
}

async function microphoneStream(): Promise<MediaStream> {
  if (recordingStream && recordingStream.active) {
    return recordingStream;
  }
  recordingStream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
  });
  return recordingStream;
}

async function monoSamplesAtWhisperRate(clip: Blob): Promise<Float32Array> {
  const encoded = await clip.arrayBuffer();
  const decoder = new AudioContext({ sampleRate: WHISPER_SAMPLE_RATE });
  try {
    const decoded = await decoder.decodeAudioData(encoded);
    return decoded.getChannelData(0).slice();
  } finally {
    await decoder.close();
  }
}

async function startRecording(): Promise<void> {
  if (currentlyRecording || !modelIsReady || transcriptionInFlight) {
    return;
  }
  try {
    const stream = await microphoneStream();
    recordedChunks = [];
    clipRecorder = new MediaRecorder(stream);
    clipRecorder.addEventListener("dataavailable", (event) => {
      if (event.data.size > 0) {
        recordedChunks.push(event.data);
      }
    });
    clipRecorder.addEventListener("stop", () => {
      void handleFinishedClip();
    });
    clipRecorder.start();
    currentlyRecording = true;
    elements.holdToTalk.classList.add("listening");
    showEngineState("Listening", "live");
    showStatus(elements.dictationStatus, "Listening, release to transcribe.");
  } catch (error) {
    showEngineState("Needs attention", "fail");
    showStatus(
      elements.dictationStatus,
      `Microphone unavailable: ${error instanceof Error ? error.message : String(error)}`,
      true,
    );
  }
}

function stopRecording(): void {
  if (!currentlyRecording || !clipRecorder) {
    return;
  }
  currentlyRecording = false;
  elements.holdToTalk.classList.remove("listening");
  showEngineState("Working", "work");
  clipRecorder.stop();
  clipRecorder = null;
}

async function handleFinishedClip(): Promise<void> {
  const clip = new Blob(recordedChunks, { type: recordedChunks[0]?.type ?? "audio/webm" });
  recordedChunks = [];
  if (clip.size === 0) {
    showEngineState("Idle", "idle");
    showStatus(elements.dictationStatus, "Nothing was recorded.");
    return;
  }
  showStatus(elements.dictationStatus, "Transcribing on this machine…");
  try {
    const audio = await monoSamplesAtWhisperRate(clip);
    if (audio.length < WHISPER_SAMPLE_RATE * SHORTEST_USEFUL_CLIP_SECONDS) {
      showEngineState("Idle", "idle");
      showStatus(elements.dictationStatus, "That clip was too short to transcribe.");
      return;
    }
    transcriptionInFlight = true;
    transcriberWorker.postMessage({ kind: "transcribe", audio }, [audio.buffer]);
  } catch (error) {
    showEngineState("Needs attention", "fail");
    showStatus(
      elements.dictationStatus,
      `Could not decode the recording: ${error instanceof Error ? error.message : String(error)}`,
      true,
    );
  }
}

async function copyTranscriptToClipboard(): Promise<void> {
  const text = elements.transcript.value;
  if (!text) {
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    showStatus(elements.clipboardStatus, "Copied. Press Cmd+V where you want it.");
  } catch {
    elements.transcript.focus();
    elements.transcript.select();
    showStatus(elements.clipboardStatus, "Selected for you. Press Cmd+C, then Cmd+V.", true);
  }
}

function showTranscribedText(rawTranscript: string): void {
  transcriptionInFlight = false;
  const polished = polish_transcript_for_the_clipboard(
    rawTranscript,
    activeCorrectionsAsJson(),
    elements.prefersBritish.checked,
  );
  if (!polished) {
    showEngineState("Idle", "idle");
    showStatus(elements.dictationStatus, "Nothing usable was heard.");
    return;
  }
  elements.transcript.value = polished;
  rememberTranscript(polished);
  showEngineState("Ready", "idle");
  showStatus(elements.dictationStatus, "Ready.");
  void copyTranscriptToClipboard();
}

function handleWorkerReply(reply: WorkerReply): void {
  if (reply.kind === "ready") {
    modelIsReady = true;
    elements.holdToTalk.disabled = false;
    elements.loadModel.disabled = false;
    elements.loadProgress.hidden = true;
    localStorage.setItem(MODEL_STORAGE_KEY, reply.modelId);
    showEngineState("Ready", "idle");
    showStatus(elements.engineStatus, `${reply.modelId} is loaded and running locally.`);
    showStatus(elements.dictationStatus, "Ready. Hold to talk.");
    return;
  }
  if (reply.kind === "progress") {
    showEngineState("Loading", "work");
    elements.loadProgress.hidden = false;
    elements.loadProgress.value = reply.percent;
    showStatus(elements.engineStatus, `Downloading ${reply.file} into the browser cache…`);
    return;
  }
  if (reply.kind === "notice") {
    showStatus(elements.engineStatus, reply.message);
    return;
  }
  if (reply.kind === "transcribed") {
    showTranscribedText(reply.text);
    return;
  }
  transcriptionInFlight = false;
  elements.loadModel.disabled = false;
  elements.loadProgress.hidden = true;
  showEngineState("Needs attention", "fail");
  showStatus(elements.engineStatus, reply.message, true);
}

function keyShouldStartDictation(event: KeyboardEvent): boolean {
  const target = event.target as HTMLElement | null;
  const typingElsewhere =
    target instanceof HTMLTextAreaElement || target instanceof HTMLInputElement;
  return event.code === "Space" && !typingElsewhere && !event.repeat && !event.metaKey;
}

function listenForHoldToTalk(): void {
  elements.holdToTalk.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    void startRecording();
  });
  for (const endEvent of ["pointerup", "pointerleave", "pointercancel"]) {
    elements.holdToTalk.addEventListener(endEvent, () => stopRecording());
  }
  window.addEventListener("keydown", (event) => {
    if (keyShouldStartDictation(event)) {
      event.preventDefault();
      void startRecording();
    }
  });
  window.addEventListener("keyup", (event) => {
    if (event.code === "Space" && currentlyRecording) {
      event.preventDefault();
      stopRecording();
    }
  });
  window.addEventListener("blur", () => stopRecording());
}

async function startRustleWeb(): Promise<void> {
  await init();
  fillModelChoices();
  corrections = storedCorrections();
  renderCorrections();
  dictationHistory = loadHistory();
  renderHistory();
  elements.prefersBritish.checked = localStorage.getItem(BRITISH_STORAGE_KEY) !== "false";

  transcriberWorker.addEventListener("message", (event: MessageEvent<WorkerReply>) => {
    handleWorkerReply(event.data);
  });

  elements.loadModel.addEventListener("click", loadSelectedModel);

  elements.copyTranscript.addEventListener("click", () => void copyTranscriptToClipboard());
  elements.clearTranscript.addEventListener("click", () => {
    elements.transcript.value = "";
    showStatus(elements.clipboardStatus, "");
  });
  elements.clearHistory.addEventListener("click", () => {
    dictationHistory = [];
    saveHistory();
    renderHistory();
  });
  elements.addCorrection.addEventListener("click", () => {
    corrections.push({ spoken: "", written: "" });
    elements.correctionsSearch.value = "";
    renderCorrections();
    const inputs = elements.correctionsList.querySelectorAll("input");
    inputs.item(inputs.length - 2)?.focus();
  });
  elements.correctionsSearch.addEventListener("input", renderCorrections);
  elements.saveSettings.addEventListener("click", () => {
    localStorage.setItem(CORRECTIONS_STORAGE_KEY, activeCorrectionsAsJson());
    localStorage.setItem(BRITISH_STORAGE_KEY, String(elements.prefersBritish.checked));
    corrections = trimmedCorrections();
    renderCorrections();
    showStatus(elements.saveNote, "Saved in this browser.");
  });
  elements.exportCorrections.addEventListener("click", exportWordsToFile);
  elements.exportHistory.addEventListener("click", exportWordsToFile);
  elements.importCorrections.addEventListener("click", () => elements.correctionsFile.click());
  elements.importHistory.addEventListener("click", () => elements.correctionsFile.click());
  elements.correctionsFile.addEventListener("change", () => {
    const file = elements.correctionsFile.files?.item(0);
    if (file) {
      void importWordsFromFile(file);
    }
  });
  elements.wordReplaceCancel.addEventListener("click", closeWordReplacement);
  elements.wordReplaceSave.addEventListener("click", saveWordReplacement);
  elements.wordReplaceTo.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      saveWordReplacement();
    } else if (event.key === "Escape") {
      closeWordReplacement();
    }
  });
  elements.prefersBritish.addEventListener("change", () => {
    localStorage.setItem(BRITISH_STORAGE_KEY, String(elements.prefersBritish.checked));
  });

  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((tab) => {
    tab.addEventListener("click", () => selectTab(tab.dataset.tab ?? "dictation"));
  });

  listenForHoldToTalk();
  loadSelectedModel();
}

void startRustleWeb();
