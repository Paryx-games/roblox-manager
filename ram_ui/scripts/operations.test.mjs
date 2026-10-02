import assert from "node:assert/strict";
import { test } from "node:test";
import { beginOperation, finishOperation, recordLaunchProgress, recordAssetProgress, operationSnapshot, clearCompletedOperations } from "../frontend/lib/operations.ts";

test("history records safe summaries and partial counts without retaining response secrets", () => {
  const id = beginOperation("save_discord_webhook");
  finishOperation(id, "save_discord_webhook", false, "synthetic-secret-webhook");
  assert.equal(operationSnapshot()[0].status, "failed");
  assert.doesNotMatch(JSON.stringify(operationSnapshot()), /synthetic-secret/);
  const batch = beginOperation("change_group_membership");
  finishOperation(batch, "change_group_membership", true, [{ ok: true }, { ok: false, message: "synthetic-secret-cookie" }]);
  assert.equal(operationSnapshot()[0].status, "partial");
  assert.match(operationSnapshot()[0].detail, /1 succeeded; 1 need attention/);
  assert.doesNotMatch(JSON.stringify(operationSnapshot()), /synthetic-secret/);
  clearCompletedOperations();
});

test("progress replaces an existing row and clear completed keeps ongoing work", () => {
  const id = "00000000-0000-4000-8000-000000000001";
  recordLaunchProgress(id, "waiting");
  recordLaunchProgress(id, "launching");
  assert.equal(operationSnapshot().filter((item) => item.id.endsWith(id)).length, 1);
  clearCompletedOperations();
  assert.equal(operationSnapshot()[0].status, "pending");
  recordLaunchProgress(id, "requested");
  assert.equal(operationSnapshot()[0].status, "requested");
  clearCompletedOperations();
  assert.equal(operationSnapshot().length, 0);
  recordAssetProgress("00000000-0000-4000-8000-000000000002", "inReview");
  recordAssetProgress("00000000-0000-4000-8000-000000000002", "rejected");
  assert.equal(operationSnapshot()[0].status, "failed");
  clearCompletedOperations();
});

test("history is bounded and unrecognised events and read-only commands stay out", () => {
  assert.equal(beginOperation("list_accounts"), null);
  assert.equal(beginOperation("__proto__"), null);
  recordLaunchProgress("00000000-0000-4000-8000-000000000001", "__proto__");
  recordLaunchProgress("synthetic-secret", "launching");
  assert.equal(operationSnapshot().length, 0);
  for (let i = 0; i < 150; i++) { const id = beginOperation("save_settings"); finishOperation(id, "save_settings", true); }
  assert.equal(operationSnapshot().length, 100);
  clearCompletedOperations();
});
