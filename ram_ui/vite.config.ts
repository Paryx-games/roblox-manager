import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const cargoManifest = readFileSync(
  fileURLToPath(new URL("../Cargo.toml", import.meta.url)),
  "utf8",
);
const version = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

if (!version) {
  throw new Error("Unable to determine RM version from Cargo.toml");
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    "import.meta.env.VITE_RM_VERSION": JSON.stringify(version),
  },
  root: "frontend",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
});
