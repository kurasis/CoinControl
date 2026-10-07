import test from "node:test";
import assert from "node:assert/strict";
import { suiteResult, missingMirrorEvidence } from "./live-results.mjs";
const report = (provider, checks) => ({ provider, checks });
const check = (name, result) => ({ name, result });
test("throttled and partial suites never become source passes", () => {
  assert.equal(
    suiteResult(report("zerion", [check("native", "PASS"), check("quota", "RATE_LIMITED")])),
    "RATE_LIMITED",
  );
  assert.equal(
    suiteResult(report("networks", [check("holdings", "PASS"), check("history", "PARTIAL")])),
    "PARTIAL",
  );
});
test("empty, failed and unrecognized results remain failures", () => {
  for (const checks of [
    [],
    [check("auth", "FAIL")],
    [check("skipped", "SKIPPED")],
    [check("quota", "RATE_LIMITED"), check("receipt", "FAIL")],
  ])
    assert.equal(suiteResult(report("zerion", checks)), "FAIL");
});
test("429 without a tested mirror is not silently accepted", () => {
  const missing = missingMirrorEvidence([report("zerion", [check("quota", "RATE_LIMITED")])]);
  assert.equal(missing.length, 7);
  assert.ok(missing.some((s) => s.includes("bsc")));
});
test("Ethereum-only success cannot stand in for Solana or BNB mirrors", () => {
  const reports = [
    report("zerion", [check("quota", "RATE_LIMITED")]),
    report("mirror-routing", [check("ethereum: mirror balance persisted", "PASS")]),
  ];
  assert.equal(missingMirrorEvidence(reports).length, 6);
});
test("real balance evidence covers the throttled source scopes without claiming history", () => {
  const routing = report(
    "mirror-routing",
    ["ethereum", "base", "arbitrum", "optimism", "polygon", "bsc", "solana"].map((n) =>
      check(`${n}: mirror balance persisted`, "PASS"),
    ),
  );
  assert.deepEqual(
    missingMirrorEvidence([report("zerion", [check("quota", "RATE_LIMITED")]), routing]),
    [],
  );
});
test("price source limits require independent actual valuation evidence", () => {
  const limited = report("livecoinwatch", [check("quota", "RATE_LIMITED")]);
  assert.equal(missingMirrorEvidence([limited]).length, 1);
  assert.deepEqual(
    missingMirrorEvidence([
      limited,
      report("price-routing", [check("Native USD valuation via available source", "PASS")]),
    ]),
    [],
  );
});
