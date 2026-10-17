import assert from "node:assert/strict";
import test from "node:test";
import { privateServerAvailabilityLabel, privateServerOwnershipLabel, privateServerOwnerLabel, runPrivateServerChecks } from "../frontend/lib/privateServerChecks.ts";

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
  assert.equal(privateServerOwnerLabel({ ownerLookup: "notListed" }, false), "Owner unknown");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "rateLimited" }, false), "Owner check limited");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "unavailable" }, false), "Owner check failed");
  assert.equal(privateServerOwnerLabel({ ownerLookup: "incomplete" }, false), "Owner unknown");
});

test("ownership and availability indicators describe independent states", () => {
  const ownedInactive = { ownership: "owned", ownerUsername: "SyntheticOwner", status: "invalid" };
  assert.equal(privateServerOwnershipLabel(ownedInactive, false), "Owned by SyntheticOwner");
  assert.match(privateServerAvailabilityLabel(ownedInactive, false), /inactive or invalid/);
  const sharedActive = { ownership: "not-owned", status: "valid", ownerLookup: "rateLimited" };
  assert.match(privateServerOwnershipLabel(sharedActive, false), /Not owned by your accounts/);
  assert.match(privateServerAvailabilityLabel(sharedActive, false), /Link active/);
  assert.match(privateServerOwnershipLabel(undefined, true), /Checking owner/);
  assert.match(privateServerAvailabilityLabel(undefined, true), /Checking link/);
});
