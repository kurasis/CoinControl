// Rate limits are expected source states. A usable independent source must
// still be evidenced; limited/skipped source checks are never reported as PASS.
const evm = ["ethereum", "base", "arbitrum", "optimism", "polygon", "bsc"];
const scopes = {
  zerion: [...evm, "solana"],
  alchemy: [...evm.filter((n) => n !== "bsc"), "solana"],
  "alchemy-solana": ["solana"],
  helius: ["solana"],
  drpc: evm,
  ankr: [...evm, "solana"],
  "ankr-advanced": evm,
  publicnode: [...evm, "solana", "tron"],
  blockscout: ["ethereum", "arbitrum", "optimism"],
  etherscan: ["ethereum", "arbitrum", "polygon"],
  chainstack: ["solana"],
  trongrid: ["tron"],
  tonapi: ["ton"],
  toncenter: ["ton"],
  esplora: ["bitcoin"],
  mempool: ["bitcoin"],
};
export function suiteResult(report) {
  const results = report?.checks?.map((c) => c.result) ?? [];
  if (!results.length || results.some((r) => !["PASS", "RATE_LIMITED", "PARTIAL"].includes(r)))
    return "FAIL";
  if (results.includes("RATE_LIMITED")) return "RATE_LIMITED";
  return results.includes("PARTIAL") ? "PARTIAL" : "PASS";
}
export function missingMirrorEvidence(reports) {
  const routing = reports.find((r) => r.provider === "mirror-routing");
  const missing = [];
  for (const report of reports) {
    if (report.provider === "ankr") {
      for (const c of report.checks.filter(
        (c) => c.result === "PARTIAL" && c.name.endsWith(" Node access unavailable"),
      )) {
        const network = c.name.split(" ")[0];
        if (
          !routing?.checks.some(
            (c) => c.result === "PASS" && c.name === `${network}: mirror balance persisted`,
          )
        ) {
          missing.push(`ankr: no successful independent ${network} balance evidence`);
        }
      }
    }
    if (!report.checks.some((c) => c.result === "RATE_LIMITED")) continue;
    if (scopes[report.provider]) {
      for (const network of scopes[report.provider]) {
        if (
          !routing?.checks.some(
            (c) => c.result === "PASS" && c.name === `${network}: mirror balance persisted`,
          )
        ) {
          missing.push(`${report.provider}: no successful independent ${network} balance evidence`);
        }
      }
    } else if (["livecoinwatch", "defillama"].includes(report.provider)) {
      const valued = reports.some(
        (r) =>
          r.provider === "price-routing" &&
          r.checks.some(
            (c) => c.result === "PASS" && c.name === "Native USD valuation via available source",
          ),
      );
      if (!valued) missing.push(`${report.provider}: no successful independent valuation evidence`);
    } else missing.push(`${report.provider}: unknown fallback requirement`);
  }
  return missing;
}
