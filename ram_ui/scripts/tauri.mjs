import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const packageDirectory = resolve(fileURLToPath(new URL("..", import.meta.url)));
const workspaceDirectory = resolve(packageDirectory, "..");
const cargoManifest = readFileSync(resolve(workspaceDirectory, "Cargo.toml"), "utf8");
const version = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

if (!version) {
  console.error("could not find the workspace version in Cargo.toml");
  process.exit(1);
}

const argumentsList = process.argv.slice(2);
const command = argumentsList[0];
const isDevelopmentBuild = command === "dev" || argumentsList.includes("--debug");
const iconName = isDevelopmentBuild
  ? "Development.ico"
  : version.includes("-alpha")
    ? "Alpha.ico"
    : version.includes("-beta")
      ? "Beta.ico"
      : "Live.ico";
const iconPath = `../../assets/logos/${iconName}`;
const configOverride = JSON.stringify({ bundle: { icon: [iconPath] } });
const tauriCli = resolve(packageDirectory, "node_modules/@tauri-apps/cli/tauri.js");

const result = spawnSync(
  process.execPath,
  [tauriCli, ...argumentsList, "--config", configOverride],
  { cwd: packageDirectory, stdio: "inherit" },
);

if (result.error) {
  console.error(`could not start Tauri: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
