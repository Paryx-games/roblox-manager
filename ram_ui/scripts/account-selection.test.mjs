import assert from "node:assert/strict";
import test from "node:test";
import { allAccountsSelected, toggleAllAccounts } from "../frontend/lib/accountSelection.ts";

test("all accounts selects eligible rows and toggles them off without changing other selections", () => {
  const original = new Set([1, 99]);
  const selected = toggleAllAccounts(original, [1, 2]);
  assert.deepEqual([...selected], [1, 99, 2]);
  assert.deepEqual([...original], [1, 99]);
  assert.equal(allAccountsSelected(selected, [1, 2]), true);
  assert.deepEqual([...toggleAllAccounts(selected, [1, 2])], [99]);
});

test("empty and partial selections never claim all accounts are selected", () => {
  assert.equal(allAccountsSelected(new Set(), []), false);
  assert.equal(allAccountsSelected(new Set([1]), [1, 2]), false);
  assert.deepEqual([...toggleAllAccounts(new Set([99]), [])], [99]);
});
