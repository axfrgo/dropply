/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_DROPPLY_UPDATE_PREVIEW_VERSION?: string;
  readonly VITE_DROPPLY_UPDATE_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
