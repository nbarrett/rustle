import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import type {
  Config,
  DictationStatusEvent,
  ModelChoice,
  ModelDownloadProgress,
} from "./types";

export function getConfig(): Promise<Config> {
  return invoke("get_config");
}

export function getAppVersion(): Promise<string> {
  return getVersion();
}

export function saveAndApplyConfig(config: Config): Promise<void> {
  return invoke("save_and_apply_config", { newConfig: config });
}

export function listMicrophones(): Promise<string[]> {
  return invoke("list_microphones");
}

export function listModels(): Promise<ModelChoice[]> {
  return invoke("list_models");
}

export function setDictationEnabled(enabled: boolean): Promise<void> {
  return invoke("set_dictation_enabled", { enabled });
}

export function getDictationEnabled(): Promise<boolean> {
  return invoke("get_dictation_enabled");
}

export function downloadModel(fileName: string, downloadUrl: string): Promise<void> {
  return invoke("download_model", { fileName, downloadUrl });
}

export function openAccessibilitySettings(): Promise<void> {
  return invoke("open_accessibility_settings");
}

export function resizeSettingsWindow(contentHeight: number): Promise<void> {
  return invoke("resize_settings_window", { contentHeight });
}

export function listenForDictationStatus(
  onEvent: (status: DictationStatusEvent) => void,
): Promise<UnlistenFn> {
  return listen<DictationStatusEvent>("dictation-status", (event) => {
    onEvent(event.payload);
  });
}

export function listenForModelDownloadProgress(
  onEvent: (progress: ModelDownloadProgress) => void,
): Promise<UnlistenFn> {
  return listen<ModelDownloadProgress>("model-download-progress", (event) => {
    onEvent(event.payload);
  });
}
