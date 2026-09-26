import assert from "node:assert/strict";
import test from "node:test";
import { mergeAccountInventories, matchesInventoryComparison, sortInventoryItems } from "../frontend/lib/inventoryBrowsing.ts";

const first = { assetId: 101, name: "Item 10", assetType: "Hat", iconUrl: null, priceRobux: null };
const second = { assetId: 102, name: "Item 2", assetType: "Gear", iconUrl: null, priceRobux: 50 };

test("merging inventories retains owners without counting duplicate entries twice", () => {
  const merged = mergeAccountInventories([
    { userId: 1, items: [first, first, second] },
    { userId: 2, items: [{ ...first, iconUrl: "https://example.invalid/item.png", priceRobux: 100 }] },
  ]);
  assert.deepEqual(merged[0].ownerIds, [1, 2]);
  assert.equal(merged[0].priceRobux, 100);
  assert.equal(merged[0].iconUrl, "https://example.invalid/item.png");
  assert.deepEqual(merged[1].ownerIds, [1]);
  assert.equal(first.priceRobux, null);
  assert.equal("ownerIds" in first, false);
});

test("comparison distinguishes items shared by all from items unique to one", () => {
  const merged = mergeAccountInventories([{ userId: 1, items: [first, second] }, { userId: 2, items: [first] }]);
  assert.deepEqual(merged.filter((item) => matchesInventoryComparison(item, "shared", 2)).map((item) => item.assetId), [101]);
  assert.deepEqual(merged.filter((item) => matchesInventoryComparison(item, "unique", 2)).map((item) => item.assetId), [102]);
  assert.equal(matchesInventoryComparison(merged[0], "shared", 3), false);
});

test("name sorting uses natural numeric order and leaves the input unchanged", () => {
  const items = mergeAccountInventories([{ userId: 1, items: [first, second] }]);
  assert.deepEqual(sortInventoryItems(items, { field: "name", direction: "ascending" }).map((item) => item.assetId), [102, 101]);
  assert.deepEqual(sortInventoryItems(items, { field: "name", direction: "descending" }).map((item) => item.assetId), [101, 102]);
  assert.deepEqual(items.map((item) => item.assetId), [101, 102]);
});

test("unavailable prices stay last in both directions and free items sort as zero", () => {
  const items = mergeAccountInventories([{ userId: 1, items: [first, second, { ...second, assetId: 103, priceRobux: 0 }] }]);
  assert.deepEqual(sortInventoryItems(items, { field: "priceRobux", direction: "ascending" }).map((item) => item.assetId), [103, 102, 101]);
  assert.deepEqual(sortInventoryItems(items, { field: "priceRobux", direction: "descending" }).map((item) => item.assetId), [102, 103, 101]);
});

test("asset ID and type sorting respect the chosen direction", () => {
  const items = mergeAccountInventories([{ userId: 1, items: [first, second] }]);
  assert.deepEqual(sortInventoryItems(items, { field: "assetId", direction: "descending" }).map((item) => item.assetId), [102, 101]);
  assert.deepEqual(sortInventoryItems(items, { field: "assetType", direction: "ascending" }).map((item) => item.assetId), [102, 101]);
});
