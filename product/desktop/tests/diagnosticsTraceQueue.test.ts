import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { diagnosticsTraceSemanticKeyForTest } from "../src/lib/diagnosticsTrace.ts";

const sharedRedactionVectors = JSON.parse(readFileSync(join(
  dirname(fileURLToPath(import.meta.url)), "..", "..", "diagnostics", "redaction_adversarial_vectors.json",
), "utf8")) as Array<{ input: string; secrets: string[]; preserve: string[] }>;

test("shared diagnostics redaction vectors cover proxy authorization and quoted header tuples", () => {
  for (const vector of sharedRedactionVectors) {
    const key = diagnosticsTraceSemanticKeyForTest("shared_redaction_probe", "warn", { message: vector.input });
    for (const secret of vector.secrets) assert.equal(key.toLowerCase().includes(secret.toLowerCase()), false);
    for (const context of vector.preserve) assert.equal(key.toLowerCase().includes(context.toLowerCase()), true);
    assert.match(key, /<redacted>/);
  }
});

test("diagnostics coalescing preserves distinct request span phase and payload rows", () => {
  const base = { request_id: "request-a", span_id: "span-a", phase: "db", count: 1, token: "secret-a" };
  const reordered = { token: "secret-b", count: 1, phase: "db", span_id: "span-a", request_id: "request-a" };
  assert.equal(
    diagnosticsTraceSemanticKeyForTest("command_phase", "info", base),
    diagnosticsTraceSemanticKeyForTest("command_phase", "info", reordered),
    "object order and redacted secret values are semantically equivalent",
  );
  for (const details of [
    { ...base, request_id: "request-b" },
    { ...base, span_id: "span-b" },
    { ...base, phase: "storage" },
    { ...base, count: 2 },
  ]) {
    assert.notEqual(
      diagnosticsTraceSemanticKeyForTest("command_phase", "info", base),
      diagnosticsTraceSemanticKeyForTest("command_phase", "info", details),
      "distinct diagnostic evidence must not coalesce",
    );
  }
});

test("diagnostics coalescing redacts bare and quoted free-text secrets without erasing context", () => {
  const first = {
    message: "password=alpha token = \"bravo two\" apikey='charlie three' api_key delta secret = \"echo five\" key=\"foxtrot six\" \"password\" = \"golf seven\" --token 'hotel eight' action=read",
  };
  const second = {
    message: "password=omega token = \"juliet two\" apikey='kilo three' api_key lima secret = \"mike five\" key=\"november six\" \"password\" = \"oscar seven\" --token 'papa eight' action=read",
  };
  assert.equal(
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", first),
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", second),
    "secret value changes should coalesce after semantic-key redaction",
  );
  assert.notEqual(
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", first),
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", { ...second, message: second.message.replace("action=read", "action=write") }),
    "non-secret context must remain distinct",
  );
});

test("diagnostics coalescing redacts spaced and combined colon secret forms", () => {
  const first = {
    message: "password : \"colon alpha\" token:colon-bravo api_key : 'colon charlie' apikey:colon-delta secret : colon-echo action:read",
  };
  const second = {
    message: "password : \"other alpha\" token:other-bravo api_key : 'other charlie' apikey:other-delta secret : other-echo action:read",
  };
  assert.equal(
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", first),
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", second),
    "colon-form secret values should coalesce after semantic-key redaction",
  );
  assert.notEqual(
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", first),
    diagnosticsTraceSemanticKeyForTest("sink_probe", "warn", { message: second.message.replace("action:read", "action:write") }),
    "non-secret colon context must remain distinct",
  );
});

test("diagnostics coalescing redacts complete authorization schemes and credentials", () => {
  const variants = [
    ["Authorization: Bearer bearer-alpha action:read", "Authorization: Bearer bearer-omega action:read"],
    ["authorization : basic basic-alpha action:read", "authorization : basic basic-omega action:read"],
    ["AUTHORIZATION:Basic compact-alpha action:read", "AUTHORIZATION:Basic compact-omega action:read"],
    ["Authorization:Bearer adjacent-alpha action:read", "Authorization:Bearer adjacent-omega action:read"],
    ["Authorization : bEaReR : malformed-alpha action:read", "Authorization : bEaReR : malformed-omega action:read"],
    ["Authorization :Bearer attached-alpha action:read", "Authorization :Bearer attached-omega action:read"],
    ["Authorization =Basic equals-alpha action:read", "Authorization =Basic equals-omega action:read"],
  ];

  for (const [first, second] of variants) {
    const firstKey = diagnosticsTraceSemanticKeyForTest("authorization_probe", "warn", { message: first });
    const secondKey = diagnosticsTraceSemanticKeyForTest("authorization_probe", "warn", { message: second });
    assert.equal(firstKey, secondKey, `authorization credentials must not affect the semantic key: ${first}`);
    assert.doesNotMatch(firstKey, /bearer|basic/i, "authorization scheme must be redacted with its credential");
    assert.doesNotMatch(firstKey, /(?:alpha|omega)/i, "authorization credential must be redacted");
    assert.match(firstKey, /action:read/, "non-secret diagnostic context must remain available");
  }

  const malformed = diagnosticsTraceSemanticKeyForTest("authorization_probe", "warn", {
    messages: ["Authorization:", "authorization :", "AUTHORIZATION:Bearer"],
  });
  assert.doesNotMatch(malformed, /bearer|basic/i, "an incomplete authorization scheme must not be persisted");
  assert.match(malformed, /<redacted>/, "malformed authorization fields still receive a redaction marker");
});
