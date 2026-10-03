import assert from "node:assert/strict";
import { test } from "node:test";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const wrapper = fileURLToPath(new URL("tauri.mjs", import.meta.url));

for (const argumentsList of [
  ["build", "--demo"],
  ["dev", "--release", "--demo"],
  ["build", "--debug", "--profile", "release", "--demo"],
  ["build", "--debug", "--profile=release", "--demo"],
  ["info", "--demo"],
]) {
  test(`reject unsafe demo invocation: ${argumentsList.join(" ")}`, () => {
    const result = spawnSync(process.execPath, [wrapper, ...argumentsList], { encoding: "utf8" });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Release demos are prohibited/);
  });
}
