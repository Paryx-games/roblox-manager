import assert from "node:assert/strict";
import test from "node:test";
import { getSelectionRectangle, selectInventoryAssets } from "../frontend/lib/inventorySelection.ts";

const tiles = [
  { assetId: 101, left: 10, top: 10, right: 50, bottom: 50 },
  { assetId: 102, left: 60, top: 10, right: 100, bottom: 50 },
  { assetId: 103, left: 10, top: 110, right: 50, bottom: 150 },
];

test("dragging in either direction selects the same intersecting tiles", () => {
  const forward = getSelectionRectangle({ x: 0, y: 0 }, { x: 80, y: 40 });
  const backward = getSelectionRectangle({ x: 80, y: 40 }, { x: 0, y: 0 });
  assert.deepEqual(forward, backward);
  assert.deepEqual([...selectInventoryAssets(backward, tiles, new Set())], [101, 102]);
});

test("touching a tile edge does not select the neighbouring tile", () => {
  const rectangle = getSelectionRectangle({ x: 50, y: 0 }, { x: 60, y: 100 });
  assert.equal(selectInventoryAssets(rectangle, tiles, new Set()).size, 0);
});

test("additive dragging retains hidden selections without mutating them", () => {
  const original = new Set([999]);
  const rectangle = getSelectionRectangle({ x: 0, y: 100 }, { x: 40, y: 140 });
  assert.deepEqual([...selectInventoryAssets(rectangle, tiles, original)], [999, 103]);
  assert.deepEqual([...original], [999]);
});
