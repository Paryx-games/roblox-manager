import assert from "node:assert/strict";
import test from "node:test";
import { committedRangeNumber, commitRangeNumberOnEvent, validRangeNumberDraft } from "../frontend/lib/rangeNumber.ts";
import { hasResourceMonitorSettingsChanged } from "../frontend/lib/resourceMonitorSettings.ts";

test("range number commit rounds and clamps values", () => {
  assert.equal(committedRangeNumber("4.6", 5, 1, 10), 5);
  assert.equal(committedRangeNumber("0", 5, 1, 10), 1);
  assert.equal(committedRangeNumber("12", 5, 1, 10), 10);
});

test("range number commit restores the current value for non-finite input", () => {
  assert.equal(committedRangeNumber("NaN", 5, 1, 10), 5);
  assert.equal(committedRangeNumber("Infinity", 5, 1, 10), 5);
});

test("range number draft accepts only in-range integers and commits on blur or Enter", () => {
  assert.equal(validRangeNumberDraft("8", 1, 10), 8);
  assert.equal(validRangeNumberDraft("8.5", 1, 10), null);
  assert.equal(validRangeNumberDraft("11", 1, 10), null);
  assert.equal(validRangeNumberDraft("", 1, 10), null);
  let commits = 0;
  const commit = () => { commits += 1; };
  commitRangeNumberOnEvent("blur", undefined, commit);
  commitRangeNumberOnEvent("keydown", "Enter", commit);
  commitRangeNumberOnEvent("keydown", "Escape", commit);
  assert.equal(commits, 2);
});

test("resource monitoring dirty state includes each editable setting", () => {
  const saved = { cpuLimitPercent: null, memoryLimitMb: null, cpuAction: "warn", memoryAction: "warn" };
  assert.equal(hasResourceMonitorSettingsChanged(saved, saved), false);
  for (const change of [
    { cpuLimitPercent: 80 },
    { memoryLimitMb: 4096 },
    { cpuAction: "kill" },
    { memoryAction: "kill" },
  ]) {
    assert.equal(hasResourceMonitorSettingsChanged({ ...saved, ...change }, saved), true);
  }
});
