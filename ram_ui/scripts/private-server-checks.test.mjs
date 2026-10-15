import assert from "node:assert/strict";
import test from "node:test";
import { privateServerOwnerLabel, runPrivateServerChecks } from "../frontend/lib/privateServerChecks.ts";

test("one failed bookmark does not stop later owner checks", async () => {
  const results = [];
  const errors = [];
  await runPrivateServerChecks([{ index: 0 }, { index: 1 }], async index => {
    if (index === 0) throw new Error("synthetic request failure");
    return { status: "valid", ownerLookup: "notListed" };
  }, { isCurrent: () => true, onResult: index => results.push(index), onError: index => errors.push(index) });
  assert.deepEqual(errors, [0]);
  assert.deepEqual(results, [1]);
});

test("cancelled and stale bookmark requests never replace current results", async () => {
  let active = true;
  let calls = 0;
  await runPrivateServerChecks([{ index: 0 }, { index: 1 }], async () => {
    calls += 1;
    active = false;
    return { status: "valid" };
  }, { isCurrent: () => active, onResult: () => assert.fail("stale result"), onError: () => assert.fail("stale error") });
  assert.equal(calls, 1);
});

test("owner labels distinguish absent metadata from failed requests", () => {
  assert.equal(privateServerOwnerLabel({ ownerLookup: "notListed" }, false), "Owner not listed for these accounts");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "rateLimited" }, false), "Owner lookup rate-limited");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "unavailable" }, false), "Owner lookup failed");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "incomplete" }, false), "Owner lookup incomplete");
});
