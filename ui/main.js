const invoke = window.__TAURI__.core.invoke;
const listen = window.__TAURI__.event.listen;

const HOTKEY_CHOICES = [
  { value: "Function", label: "🌐 Globe (fn)" },
  { value: "RightOption", label: "Right Option" },
  { value: "RightControl", label: "Right Control" },
  { value: "F8", label: "F8" },
  { value: "F9", label: "F9" },
];

const elements = {
  statusPill: document.getElementById("status-pill"),
  dictationToggle: document.getElementById("dictation-toggle"),
  dictationCaption: document.getElementById("dictation-caption"),
  hotkeySelect: document.getElementById("hotkey-select"),
  microphoneSelect: document.getElementById("microphone-select"),
  modelSelect: document.getElementById("model-select"),
  modelDownload: document.getElementById("model-download"),
  downloadStatus: document.getElementById("download-status"),
  launchToggle: document.getElementById("launch-toggle"),
  liveTranscript: document.getElementById("live-transcript"),
  correctionsList: document.getElementById("corrections-list"),
  addCorrection: document.getElementById("add-correction"),
  saveButton: document.getElementById("save-button"),
  saveNote: document.getElementById("save-note"),
};

let selectedModelFileName = "ggml-base.en.bin";
let corrections = [];

async function fitWindowToContent() {
  try {
    const contentHeight = document.documentElement.scrollHeight;
    await invoke("resize_settings_window", { contentHeight });
  } catch (error) {}
}

function renderCorrections() {
  elements.correctionsList.innerHTML = "";
  corrections.forEach((rule, index) => {
    const row = document.createElement("div");
    row.className = "correction-row";

    const spoken = document.createElement("input");
    spoken.type = "text";
    spoken.placeholder = "heard as…";
    spoken.value = rule.spoken;
    spoken.addEventListener("input", (event) => {
      corrections[index].spoken = event.target.value;
    });

    const arrow = document.createElement("span");
    arrow.className = "correction-arrow";
    arrow.textContent = "→";

    const written = document.createElement("input");
    written.type = "text";
    written.placeholder = "write as…";
    written.value = rule.written;
    written.addEventListener("input", (event) => {
      corrections[index].written = event.target.value;
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

function populateHotkeys(selectedValue) {
  elements.hotkeySelect.innerHTML = "";
  for (const choice of HOTKEY_CHOICES) {
    const option = document.createElement("option");
    option.value = choice.value;
    option.textContent = choice.label;
    if (choice.value === selectedValue) {
      option.selected = true;
    }
    elements.hotkeySelect.appendChild(option);
  }
}

async function populateMicrophones(selectedName) {
  const names = await invoke("list_microphones").catch(() => []);
  elements.microphoneSelect.innerHTML = "";

  const defaultOption = document.createElement("option");
  defaultOption.value = "";
  defaultOption.textContent = "System default";
  elements.microphoneSelect.appendChild(defaultOption);

  for (const name of names) {
    const option = document.createElement("option");
    option.value = name;
    option.textContent = name;
    if (name === selectedName) {
      option.selected = true;
    }
    elements.microphoneSelect.appendChild(option);
  }
}

let modelCatalog = [];

async function populateModels() {
  modelCatalog = await invoke("list_models").catch(() => []);
  elements.modelSelect.innerHTML = "";

  for (const model of modelCatalog) {
    const option = document.createElement("option");
    option.value = model.file_name;
    option.textContent = model.installed
      ? model.label + " · installed"
      : model.label + " · " + model.approximate_download + " (not downloaded)";
    if (model.file_name === selectedModelFileName) {
      option.selected = true;
    }
    elements.modelSelect.appendChild(option);
  }

  updateModelDownloadButton();
  fitWindowToContent();
}

function selectedModel() {
  return modelCatalog.find((model) => model.file_name === elements.modelSelect.value);
}

function updateModelDownloadButton() {
  const model = selectedModel();
  elements.modelDownload.hidden = !(model && !model.installed);
}

async function downloadSelectedModel() {
  const model = selectedModel();
  if (!model) {
    return;
  }
  elements.modelDownload.disabled = true;
  elements.modelDownload.textContent = "Downloading…";
  elements.downloadStatus.textContent = "Starting " + model.label + "…";
  try {
    await invoke("download_model", {
      fileName: model.file_name,
      downloadUrl: model.download_url,
    });
    elements.downloadStatus.textContent = model.label + " installed.";
    await populateModels();
  } catch (error) {
    elements.downloadStatus.textContent = "Download failed: " + error;
  } finally {
    elements.modelDownload.disabled = false;
    elements.modelDownload.textContent = "Download";
  }
}

function setStatus(text, className) {
  elements.statusPill.textContent = text;
  elements.statusPill.className = "status-pill " + className;
}

function showTranscript(text, live) {
  elements.liveTranscript.value = text;
  elements.liveTranscript.classList.toggle("is-live", live);
}

function applyStatusEvent(payload) {
  switch (payload.kind) {
    case "listening":
      setStatus("Listening", "status-live");
      showTranscript("Listening…", true);
      break;
    case "partial":
      setStatus("Listening", "status-live");
      showTranscript(payload.text || "", true);
      break;
    case "transcribing":
      setStatus("Transcribing", "status-work");
      break;
    case "typed":
      setStatus("Typed", "status-live");
      showTranscript(payload.text || "", false);
      break;
    case "failed":
      setStatus("Error", "status-fail");
      elements.saveNote.textContent = payload.text || "";
      break;
    default:
      setStatus("Idle", "status-idle");
  }
}

async function saveSettings() {
  const microphoneValue = elements.microphoneSelect.value;

  const chosenModel = selectedModel();
  const installedFallback =
    (modelCatalog.find((model) => model.installed) || {}).file_name ||
    "ggml-base.en.bin";
  const modelFileName =
    chosenModel && chosenModel.installed ? chosenModel.file_name : installedFallback;
  if (chosenModel && !chosenModel.installed) {
    elements.saveNote.textContent =
      chosenModel.label + " isn't downloaded yet, keeping " + modelFileName;
  }

  const config = {
    hotkey: elements.hotkeySelect.value,
    model_file_name: modelFileName,
    input_device_name: microphoneValue === "" ? null : microphoneValue,
    launch_at_login: elements.launchToggle.checked,
    corrections: corrections
      .map((rule) => ({ spoken: rule.spoken.trim(), written: rule.written.trim() }))
      .filter((rule) => rule.spoken !== ""),
  };

  elements.saveButton.disabled = true;
  try {
    await invoke("save_and_apply_config", { newConfig: config });
    elements.saveNote.textContent = "Saved.";
  } catch (error) {
    elements.saveNote.textContent = "Could not save: " + error;
  } finally {
    elements.saveButton.disabled = false;
  }
}

async function initialise() {
  const config = await invoke("get_config");
  selectedModelFileName = config.model_file_name;

  populateHotkeys(config.hotkey);
  await populateMicrophones(config.input_device_name);
  await populateModels();
  elements.launchToggle.checked = config.launch_at_login;

  elements.modelSelect.addEventListener("change", () => {
    selectedModelFileName = elements.modelSelect.value;
    updateModelDownloadButton();
  });
  elements.modelDownload.addEventListener("click", downloadSelectedModel);

  corrections = (config.corrections || []).map((rule) => ({
    spoken: rule.spoken,
    written: rule.written,
  }));
  renderCorrections();
  elements.addCorrection.addEventListener("click", () => {
    corrections.push({ spoken: "", written: "" });
    renderCorrections();
  });

  const enabled = await invoke("get_dictation_enabled").catch(() => true);
  elements.dictationToggle.checked = enabled;
  elements.dictationCaption.textContent = enabled
    ? "Listening for the hotkey"
    : "Paused";

  elements.dictationToggle.addEventListener("change", async () => {
    const on = elements.dictationToggle.checked;
    await invoke("set_dictation_enabled", { enabled: on });
    elements.dictationCaption.textContent = on
      ? "Listening for the hotkey"
      : "Paused";
  });

  elements.saveButton.addEventListener("click", saveSettings);

  await listen("dictation-status", (event) => applyStatusEvent(event.payload));
  await listen("model-download-progress", (event) => {
    const percent = event.payload.percent;
    if (typeof percent === "number") {
      elements.downloadStatus.textContent =
        "Downloading… " + percent.toFixed(0) + "%";
    }
  });

  window.addEventListener("focus", fitWindowToContent);
  fitWindowToContent();
}

initialise();
