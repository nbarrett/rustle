/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_RUSTLE_MODEL_HOST?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
