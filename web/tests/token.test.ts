// `node --test tests/token.test.ts`: the fragment handling of src/lib/token.ts.
import { test } from "node:test";
import assert from "node:assert/strict";
import { splitFragment } from "../src/lib/token.ts";

test("the URL rxp ui prints yields the token and an empty hash", () => {
  assert.deepEqual(splitFragment("#token=abc-_123"), { token: "abc-_123", rest: "" });
});

test("a token after a route leaves the route", () => {
  assert.deepEqual(splitFragment("#/cards&token=abc"), { token: "abc", rest: "#/cards" });
  assert.deepEqual(splitFragment("#/builder?panel=x&token=abc&y=1"), { token: "abc", rest: "#/builder?panel=x&y=1" });
});

test("a percent-encoded token is decoded", () => {
  assert.deepEqual(splitFragment("#token=a%2Bb%3D"), { token: "a+b=", rest: "" });
});

test("no token leaves the hash as it is", () => {
  assert.deepEqual(splitFragment(""), { token: null, rest: "" });
  assert.deepEqual(splitFragment("#/wall"), { token: null, rest: "#/wall" });
  assert.deepEqual(splitFragment("#token="), { token: null, rest: "#token=" });
  assert.deepEqual(splitFragment("#/x?mytoken=1"), { token: null, rest: "#/x?mytoken=1" });
});
