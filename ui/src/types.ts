export type HotkeyChoice =
  | "Function"
  | "RightOption"
  | "RightControl"
  | "F8"
  | "F9";

export type Correction = {
  spoken: string;
  written: string;
};

export type Config = {
  hotkey: HotkeyChoice;
  model_file_name: string;
  input_device_name: string | null;
  launch_at_login: boolean;
  press_enter_on_release: boolean;
  corrections: Correction[];
};

export type ModelChoice = {
  label: string;
  file_name: string;
  approximate_download: string;
  download_url: string;
  installed: boolean;
};

export type HistoryEntry = {
  text: string;
  time: string;
};

export type DictationStatusEvent =
  | { kind: "idle" }
  | { kind: "listening" }
  | { kind: "transcribing" }
  | { kind: "partial"; text: string }
  | { kind: "typed"; text: string }
  | { kind: "settings_preview"; text: string }
  | { kind: "failed"; text: string }
  | { kind: "needs_permission"; text: string };

export type ModelDownloadProgress = {
  received: number;
  total: number | null;
  percent: number | null;
};

export type HotkeyOption = {
  value: HotkeyChoice;
  label: string;
};
