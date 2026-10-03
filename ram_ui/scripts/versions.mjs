import { readFileSync, writeFileSync, existsSync, unlinkSync } from "node:fs";
import { resolve, relative } from "node:path";
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline/promises";

const trackedFiles = [
  "Cargo.toml",
  "Cargo.lock",
  "ram_ui/package.json",
  "ram_ui/src-tauri/tauri.conf.json",
  "ram_ui/pnpm-lock.yaml",
];

export function readWorkspaceVersion(manifest) {
  const sections = [...manifest.matchAll(/^\[workspace\.package\][ \t]*(?:#[^\r\n]*)?\r?\n([^]*?)(?=^\[|$(?![^]))/gm)];
  if (sections.length !== 1) {
    throw new Error("Expected exactly one [workspace.package] table in Cargo.toml.");
  }
  const section = sections[0];
  const versions = [...section[1].matchAll(/^version[ \t]*=[ \t]*(["'])([^"'\r\n]+)\1[ \t]*(?:#[^\r\n]*)?\r?$/gm)];
  if (versions.length !== 1) {
    throw new Error("Expected one string version in [workspace.package] in Cargo.toml.");
  }
  const declaration = versions[0];
  return {
    version: declaration[2],
    offset: section.index + section[0].indexOf(section[1]) + declaration.index + declaration[0].indexOf(declaration[1]) + 1,
  };
}

export function validateVersion(version) {
  const match = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.(0|[1-9]\d*))?$/.exec(version);
  if (!match || match[0] !== version || match[1] === "0") {
    throw new Error("Use MAJOR.MINOR.PATCH (major >= 1), optionally followed by -alpha.N, -beta.N or -rc.N. No v prefix, leading zeroes or build metadata.");
  }
  if (match.slice(1, 4).some((component) => BigInt(component) > 65535n) || (match[5] && BigInt(match[5]) > 65535n)) {
    throw new Error("Version components must be <= 65535 for Windows version resources.");
  }
  return version;
}

export function runCommand(command, argumentsList, directory) {
  const isWindowsPnpm = process.platform === "win32" && command === "pnpm";
  if (isWindowsPnpm && JSON.stringify(argumentsList) !== JSON.stringify(["install", "--lockfile-only", "--frozen-lockfile", "--offline", "--ignore-scripts"])) {
    throw new Error("Refusing unexpected pnpm shell arguments.");
  }
  const result = spawnSync(isWindowsPnpm ? process.env.ComSpec ?? "cmd.exe" : command, isWindowsPnpm ? ["/d", "/s", "/c", "pnpm install --lockfile-only --frozen-lockfile --offline --ignore-scripts"] : argumentsList, {
    cwd: directory,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} ${argumentsList.join(" ")} failed: ${result.error?.message ?? (result.stderr || result.stdout || `exit ${result.status}`)}`);
  }
  return result.stdout;
}

function replaceJsonVersion(source, version, options = {}) {
  const original = JSON.parse(source);
  if (options.remove && !Object.hasOwn(original, "version")) return source;
  const declarations = [...source.matchAll(/^[ \t]*"version"[ \t]*:[ \t]*"[^"\r\n]*"[ \t]*,?[ \t]*\r?\n/gm)];
  if (declarations.length !== 1) {
    throw new Error("Expected one version declaration on its own line in the JSON manifest; refusing an ambiguous edit.");
  }
  const declaration = declarations[0];
  const replacement = options.remove ? "" : declaration[0].replace(/(:[ \t]*)"[^"\r\n]*"/, `$1"${version}"`);
  const updated = source.slice(0, declaration.index) + replacement + source.slice(declaration.index + declaration[0].length);
  const expected = { ...original };
  if (options.remove) delete expected.version;
  else expected.version = version;
  if (JSON.stringify(JSON.parse(updated)) !== JSON.stringify(expected)) {
    throw new Error("JSON version edit would alter other fields; refusing to write it.");
  }
  return updated;
}

export function createVersionTools(directory, options = {}) {
  const execute = options.runCommand ?? runCommand;
  const read = (path) => readFileSync(resolve(directory, path), "utf8");
  const current = () => validateVersion(readWorkspaceVersion(read("Cargo.toml")).version);
  const metadata = (argumentsList) => JSON.parse(execute("cargo", ["metadata", "--format-version", "1", "--offline", ...argumentsList], directory));

  function validateReleaseCandidate(version) {
    if (!version.includes("-rc.")) return;
    const baseVersion = version.split("-")[0];
    const tags = execute("git", ["tag", "--list", `v${baseVersion}-alpha.*`, `v${baseVersion}-beta.*`], directory);
    if (!tags.split(/\r?\n/).some((tag) => tag.startsWith(`v${baseVersion}-`) && /^v\d+\.\d+\.\d+-(alpha|beta)\.(0|[1-9]\d*)$/.test(tag))) {
      throw new Error(`VERSIONING.md requires an earlier alpha or beta tag for ${baseVersion} before an rc. Fetch release tags first if they are missing locally.`);
    }
  }

  function check(options = {}) {
    const version = current();
    const packageVersion = JSON.parse(read("ram_ui/package.json")).version;
    if (packageVersion !== version) {
      throw new Error(`ram_ui/package.json is ${packageVersion}; Cargo.toml is ${version}. Run pnpm version:sync.`);
    }
    if (Object.hasOwn(JSON.parse(read("ram_ui/src-tauri/tauri.conf.json")), "version")) {
      throw new Error("tauri.conf.json must omit version to inherit Cargo's version. Run pnpm version:sync.");
    }
    const resolved = metadata(["--no-deps", "--locked"]);
    if (options.includeDependencies) execute("cargo", ["update", "--workspace", "--offline", "--locked"], directory);
    for (const identifier of resolved.workspace_members) {
      const member = resolved.packages.find((entry) => entry.id === identifier);
      if (!member || member.version !== version) throw new Error(`Cargo workspace member ${identifier} does not resolve to ${version}.`);
      const memberManifest = readFileSync(member.manifest_path, "utf8");
      if (!/^version\.workspace[ \t]*=[ \t]*true[ \t]*(?:#[^\r\n]*)?\r?$/m.test(memberManifest)) {
        throw new Error(`${relative(directory, member.manifest_path)} must use version.workspace = true.`);
      }
    }
    const lockfile = read("Cargo.lock");
    for (const identifier of resolved.workspace_members) {
      const member = resolved.packages.find((entry) => entry.id === identifier);
      const entries = lockfile.split(/^\[\[package\]\]\r?\n/m).filter((entry) => entry.startsWith(`name = "${member.name}"\n`) || entry.startsWith(`name = "${member.name}"\r\n`));
      if (entries.length !== 1 || !entries[0].startsWith(`name = "${member.name}"${lockfile.includes("\r\n") ? "\r\n" : "\n"}version = "${version}"`)) {
        throw new Error(`Cargo.lock has a stale or missing version for ${member.name}. Run pnpm version:sync.`);
      }
    }
    if (options.releaseTag && options.releaseTag !== `v${version}`) {
      throw new Error(`Release tag ${options.releaseTag} does not match Cargo.toml v${version}.`);
    }
    return version;
  }

  function synchronize(options = {}) {
    const oldVersion = current();
    const version = validateVersion(options.version ?? oldVersion);
    validateReleaseCandidate(version);
    const originals = new Map(trackedFiles.map((path) => [path, existsSync(resolve(directory, path)) ? readFileSync(resolve(directory, path)) : null]));
    let step = "prepare manifest edits";
    try {
      const manifest = read("Cargo.toml");
      const declaration = readWorkspaceVersion(manifest);
      const updatedManifest = manifest.slice(0, declaration.offset) + version + manifest.slice(declaration.offset + declaration.version.length);
      const updates = new Map([
        ["Cargo.toml", updatedManifest],
        ["ram_ui/package.json", replaceJsonVersion(read("ram_ui/package.json"), version)],
        ["ram_ui/src-tauri/tauri.conf.json", replaceJsonVersion(read("ram_ui/src-tauri/tauri.conf.json"), version, { remove: true })],
      ]);
      step = "write manifests";
      for (const [path, content] of updates) {
        if (read(path) !== content) writeFileSync(resolve(directory, path), content);
      }
      step = "update Cargo.lock with Cargo";
      execute("cargo", ["update", "--workspace", "--offline"], directory);
      step = "verify pnpm lockfile metadata";
      execute("pnpm", ["install", "--lockfile-only", "--frozen-lockfile", "--offline", "--ignore-scripts"], resolve(directory, "ram_ui"));
      step = "validate synchronized versions and locked Cargo dependencies";
      check({ includeDependencies: true });
      const changedFiles = trackedFiles.filter((path) => {
        const before = originals.get(path);
        const after = existsSync(resolve(directory, path)) ? readFileSync(resolve(directory, path)) : null;
        return before === null ? after !== null : after === null || !before.equals(after);
      });
      return { oldVersion, version, changedFiles };
    } catch (error) {
      const rollbackErrors = [];
      for (const [path, content] of originals) {
        try {
          const absolutePath = resolve(directory, path);
          if (content === null) {
            if (existsSync(absolutePath)) unlinkSync(absolutePath);
          } else if (!existsSync(absolutePath) || !content.equals(readFileSync(absolutePath))) {
            writeFileSync(absolutePath, content);
          }
        } catch (rollbackError) {
          rollbackErrors.push(`${path}: ${rollbackError.message}`);
        }
      }
      throw new Error(`${step} failed: ${error.message}\n${rollbackErrors.length ? `ROLLBACK FAILED: ${rollbackErrors.join("; ")}. Inspect these files before continuing.` : "Original manifests and lockfiles restored."}`);
    }
  }

  async function bump() {
    const oldVersion = current();
    const terminal = createInterface({ input: process.stdin, output: process.stdout });
    try {
      console.log(`Current canonical version: ${oldVersion}`);
      const version = validateVersion((await terminal.question("New version: ")).trim());
      validateReleaseCandidate(version);
      console.log(`${oldVersion} -> ${version}`);
      const answer = await terminal.question("Apply this version? [y/N]: ");
      if (answer.trim().toLowerCase() !== "y") {
        console.log("Cancelled. No files changed.");
        return;
      }
      printSummary(synchronize({ version }));
    } finally {
      terminal.close();
    }
  }

  return { current, check, synchronize, bump };
}

export function printSummary({ oldVersion, version, changedFiles }) {
  console.log(`Version verified: ${oldVersion} -> ${version}`);
  console.log(`Synchronized files: ${changedFiles.length ? changedFiles.join(", ") : "none (already synchronized)"}`);
  console.log("Rust crates and Tauri inherit Cargo.toml; package.json matches; Cargo and pnpm lockfiles verified.");
}
