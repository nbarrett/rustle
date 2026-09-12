import { pipeline, env } from "@huggingface/transformers";

type LoadRequest = { kind: "load"; modelId: string };
type TranscribeRequest = { kind: "transcribe"; audio: Float32Array };
type WorkerRequest = LoadRequest | TranscribeRequest;

const selfHostedModelOrigin = import.meta.env.VITE_RUSTLE_MODEL_HOST ?? "";

env.allowLocalModels = false;
if (selfHostedModelOrigin) {
  env.remoteHost = selfHostedModelOrigin;
}

let speechRecogniser: unknown = null;
let loadedModelId = "";

function reportProgress(event: { status?: string; progress?: number; file?: string }): void {
  if (event.status === "progress" && typeof event.progress === "number") {
    self.postMessage({ kind: "progress", percent: event.progress, file: event.file ?? "" });
  }
}

async function buildSpeechRecogniserPreferringWebGpu(modelId: string): Promise<unknown> {
  try {
    return await pipeline("automatic-speech-recognition", modelId, {
      device: "webgpu",
      dtype: "fp32",
      progress_callback: reportProgress,
    });
  } catch {
    self.postMessage({ kind: "notice", message: "WebGPU unavailable, falling back to WebAssembly." });
    return await pipeline("automatic-speech-recognition", modelId, {
      progress_callback: reportProgress,
    });
  }
}

async function loadModelOnce(modelId: string): Promise<void> {
  if (speechRecogniser && loadedModelId === modelId) {
    self.postMessage({ kind: "ready", modelId });
    return;
  }
  speechRecogniser = await buildSpeechRecogniserPreferringWebGpu(modelId);
  loadedModelId = modelId;
  self.postMessage({ kind: "ready", modelId });
}

async function transcribeClip(audio: Float32Array): Promise<void> {
  if (!speechRecogniser) {
    self.postMessage({ kind: "failed", message: "No model is loaded yet." });
    return;
  }
  const recognise = speechRecogniser as (
    input: Float32Array,
    options: Record<string, unknown>,
  ) => Promise<{ text: string }>;
  const outcome = await recognise(audio, { chunk_length_s: 30, stride_length_s: 5 });
  self.postMessage({ kind: "transcribed", text: outcome.text ?? "" });
}

self.addEventListener("message", (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  const work = request.kind === "load" ? loadModelOnce(request.modelId) : transcribeClip(request.audio);
  work.catch((error: unknown) => {
    self.postMessage({ kind: "failed", message: error instanceof Error ? error.message : String(error) });
  });
});
