import assert from "node:assert/strict";
import { test } from "node:test";
import { parseLaunchDestination } from "../frontend/lib/launchDestination.ts";

test("place IDs and public Roblox game URLs resolve to exact safe IDs", () => {
  for (const value of ["123", " 123 ", "https://www.roblox.com/games/123/Game", "https://roblox.com/en-us/games/123", "https://www.roblox.com/games/123/"]) {
    assert.equal(parseLaunchDestination(value), 123);
  }
});

test("reject unsafe IDs, lookalike hosts, credentials and private invite links", () => {
  for (const value of ["", "0", "-1", "1.5", "123abc", "9007199254740992", "http://roblox.com/games/123", "https://evil.test/games/123", "https://roblox.com.evil.test/games/123", "https://user:password@roblox.com/games/123", "https://roblox.com:8443/games/123", "https://roblox.com/games/123?privateServerLinkCode=synthetic", "https://roblox.com/share?code=synthetic", "https://roblox.com/games/123#secret", "https://roblox.com/games/123/extra/path"]) {
    assert.equal(parseLaunchDestination(value), null, value);
  }
});
