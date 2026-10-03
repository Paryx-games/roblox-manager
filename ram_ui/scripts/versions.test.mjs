import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createVersionTools, readWorkspaceVersion, validateVersion, runCommand } from "./versions.mjs";

function createFixture(context) {
  const directory = mkdtempSync(resolve(tmpdir(), "rm-version-test-"));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  mkdirSync(resolve(directory, "ram_ui/src-tauri/src"), { recursive: true });
  writeFileSync(resolve(directory, "Cargo.toml"), '[workspace]\r\nmembers = ["ram_ui/src-tauri"]\r\nresolver = "2"\r\n\r\n[workspace.package]\r\nversion = "2.0.0-beta.2" # canonical\r\n\r\n[workspace.metadata]\r\nversion = "9.9.9"\r\n');
  writeFileSync(resolve(directory, "ram_ui/src-tauri/Cargo.toml"), '[package]\nname = "version_fixture"\nversion.workspace = true\nedition = "2021"\n');
  writeFileSync(resolve(directory, "ram_ui/src-tauri/src/lib.rs"), "");
  writeFileSync(resolve(directory, "ram_ui/package.json"), '{\r\n  "name": "version-fixture",\r\n  "version": "2.0.0-beta.2",\r\n  "private": true\r\n}\r\n');
  writeFileSync(resolve(directory, "ram_ui/src-tauri/tauri.conf.json"), '{\r\n  "productName": "Fixture",\r\n  "version": "2.0.0-beta.2",\r\n  "identifier": "com.fixture.test"\r\n}\r\n');
  writeFileSync(resolve(directory, "ram_ui/pnpm-lock.yaml"), "lockfileVersion: '9.0'\nsettings:\n  autoInstallPeers: true\n  excludeLinksFromLockfile: false\nimporters:\n  .: {}\n");
  writeFileSync(resolve(directory, "ram_ui/pnpm-workspace.yaml"), "allowBuilds: {}\n");
  runCommand("cargo", ["generate-lockfile", "--offline"], directory);
  return directory;
}

const paths = ["Cargo.toml", "Cargo.lock", "ram_ui/package.json", "ram_ui/src-tauri/tauri.conf.json", "ram_ui/pnpm-lock.yaml"];
const snapshot = (directory) => paths.map((path) => readFileSync(resolve(directory, path)));

function copyTooling(directory) {
  mkdirSync(resolve(directory, "ram_ui/scripts"), { recursive: true });
  for (const name of ["tauri.mjs", "versions.mjs"]) {
    writeFileSync(resolve(directory, "ram_ui/scripts", name), readFileSync(new URL(name, import.meta.url)));
  }
  const repositoryDirectory = resolve(fileURLToPath(new URL("../../", import.meta.url)));
  writeFileSync(resolve(directory, "bump-version.bat"), readFileSync(resolve(repositoryDirectory, "bump-version.bat")));
}

test("only the canonical Cargo table is selected, including CRLF and comments", () => {
  const source = '[package]\nversion = "9.9.9"\n[workspace.package]\nversion = \'2.0.0\' # release\n';
  const result = readWorkspaceVersion(source);
  assert.equal(result.version, "2.0.0");
  assert.equal(source.slice(result.offset, result.offset + result.version.length), "2.0.0");
  assert.throws(() => readWorkspaceVersion('[workspace.package]\nversion = "2.0.0"\nversion = "2.1.0"\n'), /one string version/);
});

test("project version syntax rejects malformed and unsupported inputs", () => {
  for (const version of ["2.0.0", "2.0.0-beta.3", "2.1.0-rc.1", "2.1.0-alpha.0"]) assert.equal(validateVersion(version), version);
  for (const version of ["", "v2.0.0", "2.0", "02.0.0", "0.1.0", "2.0.0-beta.03", "2.0.0-beta", "2.0.0-preview.1", "2.0.0+build", "2.0.0\n", "65536.0.0", "2.0.0-beta.65536", "2.0.0 & echo unsafe"]) {
    assert.throws(() => validateVersion(version));
  }
});

for (const version of ["2.0.0", "2.0.0-beta.3"]) {
  test(`set ${version}, verify real Cargo/pnpm locks and preserve unrelated content`, (context) => {
    const directory = createFixture(context);
    const tools = createVersionTools(directory);
    const result = tools.synchronize({ version });
    assert.equal(result.oldVersion, "2.0.0-beta.2");
    assert.equal(tools.check({ includeDependencies: true }), version);
    const manifest = readFileSync(resolve(directory, "Cargo.toml"), "utf8");
    assert.ok(manifest.includes(`version = "${version}" # canonical\r\n`));
    assert.ok(manifest.includes('version = "9.9.9"'));
    assert.ok(readFileSync(resolve(directory, "ram_ui/package.json"), "utf8").includes(`"version": "${version}",\r\n`));
    assert.equal(JSON.parse(readFileSync(resolve(directory, "ram_ui/src-tauri/tauri.conf.json"))).version, undefined);
    const before = snapshot(directory);
    const manifests = paths.filter((path) => !path.includes("lock"));
    const timestamps = manifests.map((path) => statSync(resolve(directory, path)).mtimeMs);
    assert.deepEqual(tools.synchronize({ version }).changedFiles, []);
    assert.deepEqual(snapshot(directory), before);
    assert.deepEqual(manifests.map((path) => statSync(resolve(directory, path)).mtimeMs), timestamps);
  });
}

test("invalid version leaves every file unchanged", (context) => {
  const directory = createFixture(context);
  const before = snapshot(directory);
  assert.throws(() => createVersionTools(directory).synchronize({ version: "2.0.bad" }), /MAJOR.MINOR.PATCH/);
  assert.deepEqual(snapshot(directory), before);
});

test("sync repairs stale package, Tauri override and Cargo.lock without bumping Cargo", (context) => {
  const directory = createFixture(context);
  const canonical = readFileSync(resolve(directory, "Cargo.toml"));
  const packagePath = resolve(directory, "ram_ui/package.json");
  writeFileSync(packagePath, readFileSync(packagePath, "utf8").replace("2.0.0-beta.2", "1.0.0"));
  const tools = createVersionTools(directory);
  assert.throws(() => tools.check(), /version:sync/);
  const result = tools.synchronize();
  assert.ok(result.changedFiles.includes("ram_ui/package.json"));
  assert.deepEqual(readFileSync(resolve(directory, "Cargo.toml")), canonical);
  assert.equal(tools.check(), "2.0.0-beta.2");
  const lockPath = resolve(directory, "Cargo.lock");
  writeFileSync(lockPath, readFileSync(lockPath, "utf8").replace("2.0.0-beta.2", "1.0.0"));
  assert.throws(() => tools.check(), /Cargo.lock/);
  assert.ok(tools.synchronize().changedFiles.includes("Cargo.lock"));
});

for (const failedStep of ["cargo", "pnpm", "validation"]) {
  test(`${failedStep} failure rolls back all manifests and lockfiles exactly`, (context) => {
    const directory = createFixture(context);
    const before = snapshot(directory);
    const tools = createVersionTools(directory, {
      runCommand(command, argumentsList, workingDirectory) {
        if ((failedStep === command && !argumentsList.includes("--locked")) || (failedStep === "validation" && argumentsList.includes("--locked"))) {
          writeFileSync(resolve(directory, "Cargo.lock"), "partial lockfile update");
          writeFileSync(resolve(directory, "ram_ui/pnpm-lock.yaml"), "partial pnpm update");
          throw new Error("simulated failure");
        }
        return runCommand(command, argumentsList, workingDirectory);
      },
    });
    assert.throws(() => tools.synchronize({ version: "2.1.0" }), /failed:.*simulated failure\nOriginal manifests and lockfiles restored/);
    assert.deepEqual(snapshot(directory), before);
  });
}

test("release candidates require earlier tags and release checks require matching tags", (context) => {
  const directory = createFixture(context);
  const before = snapshot(directory);
  const tools = createVersionTools(directory, {
    runCommand(command, argumentsList, workingDirectory) {
      return command === "git" ? "" : runCommand(command, argumentsList, workingDirectory);
    },
  });
  assert.throws(() => tools.synchronize({ version: "2.1.0-rc.1" }), /earlier alpha or beta/);
  assert.deepEqual(snapshot(directory), before);
  const taggedTools = createVersionTools(directory, {
    runCommand(command, argumentsList, workingDirectory) {
      return command === "git" ? "v2.1.0-beta.1\n" : runCommand(command, argumentsList, workingDirectory);
    },
  });
  taggedTools.synchronize({ version: "2.1.0-rc.1" });
  assert.equal(taggedTools.check({ releaseTag: "v2.1.0-rc.1" }), "2.1.0-rc.1");
  assert.throws(() => taggedTools.check({ releaseTag: "v2.1.0" }), /does not match/);
});

test("CLI rejects malformed versions with non-zero status and no edits", (context) => {
  const directory = createFixture(context);
  copyTooling(directory);
  const before = snapshot(directory);
  const result = spawnSync(process.execPath, [resolve(directory, "ram_ui/scripts/tauri.mjs"), "version:set", "2.0.bad"], { encoding: "utf8" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Version tooling failed:.*MAJOR.MINOR.PATCH/);
  assert.deepEqual(snapshot(directory), before);
});

test("real frozen pnpm lockfile failure rolls back the bump", (context) => {
  const directory = createFixture(context);
  const packagePath = resolve(directory, "ram_ui/package.json");
  const source = readFileSync(packagePath, "utf8");
  writeFileSync(packagePath, source.replace('"private": true', '"private": true,\r\n  "dependencies": { "is-number": "7.0.0" }'));
  const before = snapshot(directory);
  assert.throws(() => createVersionTools(directory).synchronize({ version: "2.1.0" }), /verify pnpm lockfile metadata failed:[^]*Original manifests and lockfiles restored/);
  assert.deepEqual(snapshot(directory), before);
});

function stopBatchProcess(child) {
  if (!child) return;
  if (child.exitCode === null && child.pid) {
    spawnSync("taskkill", ["/pid", String(child.pid), "/t", "/f"], { windowsHide: true, timeout: 5000 });
  }
  child.stdin.destroy();
  child.stdout.destroy();
  child.stderr.destroy();
}

test("Windows batch cleanup terminates a waiting Node descendant", { skip: process.platform !== "win32", timeout: 10000 }, async (context) => {
  let child;
  context.after(() => stopBatchProcess(child));
  const directory = createFixture(context);
  copyTooling(directory);
  writeFileSync(resolve(directory, "ram_ui/scripts/tauri.mjs"), "process.stdout.write(String(process.pid)); setInterval(() => {}, 1000);");
  child = spawn(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", "bump-version.bat"], { cwd: directory, windowsHide: true });
  const descendantProcessId = await new Promise((resolveProcessId, reject) => {
    child.stdout.once("data", (chunk) => resolveProcessId(Number(chunk.toString())));
    child.once("error", reject);
  });
  assert.ok(Number.isInteger(descendantProcessId) && descendantProcessId > 0);
  const closed = new Promise((resolveClose) => child.once("close", resolveClose));
  stopBatchProcess(child);
  await closed;
  assert.throws(() => process.kill(descendantProcessId, 0), { code: "ESRCH" });
});

for (const scenario of [
  { version: "2.0.0-beta.3", answer: "y", status: 0, expected: /Version verified: 2.0.0-beta.2 -> 2.0.0-beta.3/ },
  { version: "2.0.0", answer: "n", status: 0, expected: /Cancelled. No files changed/ },
  { version: "bad", status: 1, expected: /Version tooling failed/ },
]) {
  test(`Windows batch interface: ${scenario.version} ${scenario.answer ?? "rejected"}`, { skip: process.platform !== "win32", timeout: 20000 }, async (context) => {
    let child;
    context.after(() => stopBatchProcess(child));
    const directory = createFixture(context);
    copyTooling(directory);
    const before = snapshot(directory);
    child = spawn(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", "bump-version.bat"], { cwd: directory, windowsHide: true });
    let output = "";
    let wasVersionSent = false;
    let wasConfirmationSent = false;
    child.stdout.on("data", (chunk) => {
      output += chunk;
      if (!wasVersionSent && output.includes("New version:")) {
        wasVersionSent = true;
        child.stdin.write(`${scenario.version}\n`);
      }
      if (!wasConfirmationSent && output.includes("Apply this version?")) {
        wasConfirmationSent = true;
        child.stdin.write(`${scenario.answer}\n`);
      }
    });
    child.stderr.on("data", (chunk) => { output += chunk; });
    const status = await new Promise((resolveStatus, reject) => {
      child.on("error", reject);
      child.on("close", resolveStatus);
    });
    assert.equal(status, scenario.status, output);
    assert.match(output, /Current canonical version: 2.0.0-beta.2/);
    assert.match(output, scenario.expected);
    if (scenario.answer === "y") assert.equal(createVersionTools(directory).check(), scenario.version);
    else assert.deepEqual(snapshot(directory), before);
  });
}
