import assert from "node:assert/strict";
import test from "node:test";
import { accountActionTargets, allAccountsSelected, toggleAllAccounts } from "../frontend/lib/accountSelection.ts";

test("single-account actions use the displayed account despite empty or stale shared selection", () => {
  const displayed = { userId: 1, canLaunch: true };
  const other = { userId: 2, canLaunch: true };
  assert.deepEqual(accountActionTargets(displayed, []), [displayed]);
  assert.deepEqual(accountActionTargets(displayed, [other]), [displayed]);
  assert.deepEqual(accountActionTargets(null, [other]), []);
});

test("bulk actions preserve all selected accounts and single-account restrictions follow the displayed account", () => {
  const restricted = { userId: 1, canLaunch: false };
  const other = { userId: 2, canLaunch: true };
  assert.deepEqual(accountActionTargets(other, [restricted, other]), [restricted, other]);
  assert.equal(accountActionTargets(restricted, [other]).every(account => account.canLaunch), false);
  assert.equal(accountActionTargets(other, [restricted]).every(account => account.canLaunch), true);
});

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
