import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createVersionTools, printSummary } from "./versions.mjs";

const packageDirectory = resolve(fileURLToPath(new URL("..", import.meta.url)));
const workspaceDirectory = resolve(packageDirectory, "..");
const argumentsList = process.argv.slice(2);
const command = argumentsList[0];
const runnerSeparator = argumentsList.indexOf("--");
const cliArguments = argumentsList.slice(0, runnerSeparator < 0 ? argumentsList.length : runnerSeparator);
const isDemo = cliArguments.includes("--demo");
if (isDemo && !((command === "dev" && !cliArguments.includes("--release")) ||
  (command === "build" && cliArguments.includes("--debug"))) ||
  (isDemo && cliArguments.some((argument) => argument === "--profile" || argument.startsWith("--profile=")))) {
  console.error("Demo mode requires dev or build --debug, without a custom profile. Release demos are prohibited.");
  process.exit(1);
}
const versions = createVersionTools(workspaceDirectory);
let version;
try {
  if (command?.startsWith("version:")) {
    if (command === "version:sync" && argumentsList.length === 1) printSummary(versions.synchronize());
    else if (command === "version:set" && argumentsList.length === 2) printSummary(versions.synchronize({ version: argumentsList[1] }));
    else if (command === "version:check" && argumentsList.length <= 2) console.log(`Version verified: ${versions.check({ releaseTag: argumentsList[1] })}`);
    else if (command === "version:bump" && argumentsList.length === 1) await versions.bump();
    else throw new Error("Usage: version:sync | version:set <version> | version:check [release-tag] | version:bump");
    process.exit(0);
  }
  version = versions.check();
} catch (error) {
  console.error(`Version tooling failed: ${error.message}`);
  process.exit(1);
}
const isDevelopmentBuild =
  command === "dev" || argumentsList.includes("--debug");
const iconName = isDevelopmentBuild
  ? "Development.ico"
  : version.includes("-alpha")
    ? "Alpha.ico"
    : version.includes("-beta")
      ? "Beta.ico"
      : "Live.ico";
const iconPath = `../../assets/logos/${iconName}`;
const configOverride = JSON.stringify({
  bundle: {
    icon: [iconPath],
    windows: {
      nsis: { installerIcon: iconPath, uninstallerIcon: iconPath },
    },
  },
});
const configuredArguments = argumentsList.filter((argument, index) =>
  !(isDemo && index < cliArguments.length && argument === "--demo"));
const runnerArgumentsIndex = configuredArguments.indexOf("--");
configuredArguments.splice(
  runnerArgumentsIndex < 0 ? configuredArguments.length : runnerArgumentsIndex,
  0,
  "--config",
  configOverride,
);
const tauriCli = resolve(
  packageDirectory,
  "node_modules/@tauri-apps/cli/tauri.js",
);

const result = spawnSync(
  process.execPath,
  [tauriCli, ...configuredArguments],
  {
    cwd: packageDirectory,
    stdio: "inherit",
    env: {
      ...process.env,
      RM_DEMO: isDemo ? "1" : "0",
      ...(isDevelopmentBuild && !process.env.RUST_LOG
        ? { RUST_LOG: "info" }
        : {}),
    },
  },
);

if (result.error) {
  console.error(`could not start Tauri: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
