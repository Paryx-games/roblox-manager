import assert from "node:assert/strict";
import test from "node:test";
import { historyCsv } from "../frontend/lib/historyExport.ts";
test("history exports quote data and prevent spreadsheet formulas", () => {
  const event = { observedAt: "2026-10-08T10:00:00Z", userId: 42, kind: "joined", location: ' =HYPERLINK("synthetic")', placeId: 1, jobId: null };
  const csv = historyCsv([event]);
  assert.ok(csv.includes('"\' =HYPERLINK(""synthetic"")"'));
  assert.ok(csv.endsWith('"1",""'));
  assert.equal(historyCsv([]), "observed_at,user_id,event,location,place_id,job_id");
});
