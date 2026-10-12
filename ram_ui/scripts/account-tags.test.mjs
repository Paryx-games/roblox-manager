import assert from "node:assert/strict";
import test from "node:test";
import { matchesTags, commonTags } from "../frontend/lib/accountTags.ts";

test("tag filters distinguish any, all, untagged and missing matches", () => {
    assert.equal(matchesTags([], []), true);
    assert.equal(matchesTags(["farm"], ["farm", "main"]), true);
    assert.equal(matchesTags(["farm"], ["farm", "main"], true), false);
    assert.equal(matchesTags(["farm", "main"], ["farm", "main"], true), true);
    assert.equal(matchesTags([], ["farm"]), false);
    assert.equal(matchesTags(["main"], ["deleted"]), false);
});

test("bulk tag editing exposes only shared tags and preserves its inputs", () => {
    const accounts = [
        { tagIds: ["main", "farm"] },
        { tagIds: ["main", "trade"] },
    ];
    assert.deepEqual(commonTags(accounts), ["main"]);
    assert.deepEqual(commonTags([]), []);
    assert.deepEqual(accounts[0].tagIds, ["main", "farm"]);
});
