interface ImportMetaEnv {
  readonly VITE_RM_VERSION: string;
  readonly DEV: boolean;
  readonly MODE: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
