import assert from "node:assert/strict";
import test from "node:test";
import { accountSectionIds, normalizeAccountLayout, moveAccountSection } from "../frontend/lib/accountLayout.ts";

test("saved account layouts ignore obsolete sections and append new sections", () => {
  assert.deepEqual(normalizeAccountLayout(["inventory", "obsolete", "inventory", 12]), ["inventory", "launch", "roblox-settings", "connections", "details"]);
  assert.deepEqual(normalizeAccountLayout(null), accountSectionIds);
  assert.deepEqual(normalizeAccountLayout({ order: [] }), accountSectionIds);
});

test("sections move in either direction without duplicating or losing cards", () => {
  const original = [...accountSectionIds];
  const bottom = moveAccountSection(original, "roblox-settings", "details");
  assert.deepEqual(bottom, ["launch", "connections", "inventory", "details", "roblox-settings"]);
  assert.deepEqual(moveAccountSection(bottom, "roblox-settings", "launch"), ["roblox-settings", "launch", "connections", "inventory", "details"]);
  assert.deepEqual(original, accountSectionIds);
});
