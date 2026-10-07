// Controlled offline evidence for native UI and production upgrade acceptance.
// This external test utility only writes a closed, otherwise empty test database.
// It is never imported by the frontend or packaged in the application.
import { DatabaseSync } from "node:sqlite";
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const ownedFixture = {
  wallets: ["Acceptance A", "Acceptance B"],
  addresses: [
    "0x0000000000000000000000000000000000000001",
    "0x0000000000000000000000000000000000000002",
  ],
  totalUsd: "5970.00",
  accountUsd: ["2970.00", "3000.00"],
  feeUsd: "30.00",
};

export function seedOwnedFixture(path, { groups = true } = {}) {
  const db = new DatabaseSync(path);
  db.exec("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;");
  const version = db.prepare("SELECT MAX(version) AS v FROM _sqlx_migrations").get().v;
  if (![6, 7, 8].includes(version))
    throw new Error("Fixture requires an app-created schema 6, 7 or 8");
  if (db.prepare("SELECT COUNT(*) AS n FROM chain_transactions").get().n !== 0)
    throw new Error("Refusing to overwrite populated portfolio evidence");
  const now = Math.floor(Date.now() / 1000);
  const receiptAt = now - 86400 * 10;
  const transferAt = now - 86400;
  const put = (sql, ...values) => db.prepare(sql).run(...values);
  try {
    db.exec("BEGIN IMMEDIATE");
    if (db.prepare("SELECT COUNT(*) AS n FROM wallets").get().n === 0) {
      for (let i = 0; i < 2; i++) {
        put(
          "INSERT INTO wallets VALUES (?,?,0,?)",
          `acceptance-wallet-${i}`,
          ownedFixture.wallets[i],
          receiptAt,
        );
        put(
          "INSERT INTO accounts (id,wallet_id,network_id,canonical_address,display_address,label,archived,created_at) VALUES (?,?,'ethereum',?,?,?,0,?)",
          `acceptance-account-${i}`,
          `acceptance-wallet-${i}`,
          ownedFixture.addresses[i],
          ownedFixture.addresses[i],
          ownedFixture.wallets[i],
          receiptAt,
        );
      }
    }
    const accounts = ownedFixture.addresses.map((address) =>
      db
        .prepare("SELECT * FROM accounts WHERE network_id='ethereum' AND canonical_address=?")
        .get(address),
    );
    if (accounts.some((a) => !a) || db.prepare("SELECT COUNT(*) AS n FROM accounts").get().n !== 2)
      throw new Error("Fixture requires exactly the two synthetic acceptance accounts");
    put(
      "INSERT INTO assets (id,network_id,asset_kind,canonical_identifier,decimals,symbol,name,verification,metadata_provider) VALUES ('ethereum:native','ethereum','native','',18,'ETH','Ether','verified','offline-acceptance')",
    );
    for (const [id, at] of [
      ["acceptance-receipt", receiptAt],
      ["acceptance-transfer", transferAt],
    ]) {
      put(
        "INSERT INTO chain_transactions (id,network_id,canonical_tx_id,occurred_at,status,source_provider) VALUES (?,'ethereum',?,?,'final','offline-acceptance')",
        id,
        id,
        at,
      );
    }
    const legs = [
      [
        "acceptance-receipt:in",
        "acceptance-receipt",
        0,
        "2000000000000000000",
        "in",
        "receive",
        null,
        receiptAt,
      ],
      [
        "acceptance-transfer:out",
        "acceptance-transfer",
        0,
        "-1000000000000000000",
        "out",
        "send",
        ownedFixture.addresses[1],
        transferAt,
      ],
      [
        "acceptance-transfer:in",
        "acceptance-transfer",
        1,
        "1000000000000000000",
        "in",
        "receive",
        ownedFixture.addresses[0],
        transferAt,
      ],
    ];
    for (const [id, transaction, account, raw, direction, operation, counterparty, at] of legs) {
      put(
        "INSERT INTO activity_legs (id,transaction_id,account_id,asset_id,signed_raw_quantity,direction,leg_type,decoding,evidence) VALUES (?,?,?,'ethereum:native',?,?,?,'interpreted',?)",
        id,
        transaction,
        accounts[account].id,
        raw,
        direction,
        operation,
        JSON.stringify({ counterparty }),
      );
      put(
        "INSERT INTO account_transactions VALUES (?,?,?,'offline-acceptance','interpreted',?)",
        accounts[account].id,
        transaction,
        operation,
        at,
      );
    }
    put(
      "INSERT INTO transaction_fees VALUES ('acceptance-fee','acceptance-transfer',?,'ethereum:native','10000000000000000','exact')",
      accounts[0].id,
    );
    for (const [i, raw] of [
      [0, "990000000000000000"],
      [1, "1000000000000000000"],
    ]) {
      put(
        "INSERT INTO balance_observations (account_id,asset_id,raw_quantity,observed_at,provider,status) VALUES (?,'ethereum:native',?,?,'offline-acceptance','fresh')",
        accounts[i].id,
        raw,
        now,
      );
      put(
        "INSERT INTO portfolio_snapshots (account_id,asset_id,at,raw_quantity,quality,valuation_version) VALUES (?,'ethereum:native',?,?,'observed',1)",
        accounts[i].id,
        transferAt,
        raw,
      );
      put(
        "INSERT INTO sync_checkpoints (id,account_id,provider,category,coverage,earliest_covered_at,updated_at) VALUES (?,?,'zerion','history','complete',?,?) ON CONFLICT(account_id,provider,category) DO UPDATE SET coverage='complete',earliest_covered_at=excluded.earliest_covered_at",
        `acceptance-checkpoint-${i}`,
        accounts[i].id,
        receiptAt,
        now,
      );
    }
    put(
      "INSERT INTO portfolio_snapshots (account_id,asset_id,at,raw_quantity,quality,valuation_version) VALUES (?,'ethereum:native',?,'2000000000000000000','observed',1)",
      accounts[0].id,
      receiptAt,
    );
    for (const [at, price, granularity, quality] of [
      [receiptAt, "2000", "day", "estimated"],
      [transferAt, "3000", "day", "estimated"],
      [now, "3000", "tick", "current"],
    ])
      put(
        "INSERT INTO prices (asset_id,provider,price_usd,requested_at,observed_at,granularity,quality) VALUES ('ethereum:native','offline-acceptance',?,?,?,?,?)",
        price,
        now,
        at,
        granularity,
        quality,
      );
    for (const [version, basis] of [
      [1, "3900"],
      [2, "4000"],
    ])
      put(
        "INSERT INTO accounting_overrides (id,target_kind,target_id,version,payload,created_at) VALUES (?,'leg','acceptance-receipt:in',?,?,?)",
        `acceptance-decision-${version}`,
        version,
        JSON.stringify({
          basis_lots: [
            { quantity: "2", basis_usd: basis, basis_kind: "known", acquired_at: receiptAt },
          ],
          note: `Offline acceptance audit ${version}`,
        }),
        receiptAt + version,
      );
    if (groups) {
      for (const [id, label, members] of [
        ["acceptance-both", "Acceptance both", accounts],
        ["acceptance-a", "Acceptance A only", [accounts[0]]],
      ]) {
        put("INSERT INTO groups VALUES (?,?,?)", id, label, now);
        for (const account of members)
          put("INSERT INTO group_wallets VALUES (?,?)", id, account.wallet_id);
      }
    }
    put(
      "INSERT INTO settings VALUES ('app',?,?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
      JSON.stringify({
        language: "en",
        theme: "dark",
        timezone: "UTC",
        privacy_mode: false,
        price_refresh_seconds: 3600,
        sweep_interval_minutes: 1440,
      }),
      now,
    );
    put(
      "INSERT INTO app_meta VALUES ('accounting_dirty','1') ON CONFLICT(key) DO UPDATE SET value='1'",
    );
    db.exec("COMMIT; PRAGMA wal_checkpoint(TRUNCATE);");
  } catch (error) {
    db.exec("ROLLBACK");
    throw error;
  } finally {
    db.close();
  }
}

export function acceptanceSnapshot(path) {
  const db = new DatabaseSync(path, { readOnly: true });
  try {
    const tables = [
      "wallets",
      "accounts",
      "groups",
      "group_wallets",
      "assets",
      "asset_preferences",
      "chain_transactions",
      "account_transactions",
      "activity_legs",
      "transaction_fees",
      "accounting_overrides",
      "portfolio_snapshots",
      "settings",
      "lots",
      "lot_consumptions",
      "leg_accounting",
      "account_accounting",
      "accounting_flows",
      "reconciliation_items",
    ];
    const fingerprints = Object.fromEntries(
      tables.map((table) => {
        const rows = db
          .prepare(`SELECT * FROM ${table}`)
          .all()
          .map((row) => JSON.stringify(row))
          .sort();
        return [
          table,
          {
            rows: rows.length,
            sha256: createHash("sha256").update(JSON.stringify(rows)).digest("hex"),
          },
        ];
      }),
    );
    const balances = db
      .prepare(
        "SELECT account_id,asset_id,raw_quantity FROM balance_observations ORDER BY account_id,asset_id,id",
      )
      .all();
    return {
      schema: db.prepare("SELECT MAX(version) AS v FROM _sqlx_migrations").get().v,
      valuationIndex:
        db
          .prepare(
            "SELECT COUNT(*) AS n FROM sqlite_master WHERE type='index' AND name='lots_valuation_remaining'",
          )
          .get().n === 1,
      integrity: db.prepare("PRAGMA integrity_check").get().integrity_check,
      accountingDirty: db.prepare("SELECT value FROM app_meta WHERE key='accounting_dirty'").get()
        ?.value,
      fingerprints,
      balances,
      feeCharges: db
        .prepare("SELECT SUM(fee_charges) AS n FROM account_accounting WHERE asset_id=''")
        .get().n,
      ownTransferLegs: db
        .prepare(
          "SELECT COUNT(*) AS n FROM leg_accounting WHERE treatment IN ('own_transfer_in','own_transfer_out')",
        )
        .get().n,
    };
  } finally {
    db.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [command, path, destination] = process.argv.slice(2);
  if (command === "seed") seedOwnedFixture(path);
  else if (command === "snapshot" && destination)
    writeFileSync(destination, JSON.stringify(acceptanceSnapshot(path), null, 2) + "\n");
  else throw new Error("Usage: acceptance-fixture.mjs seed PROFILE | snapshot PROFILE REPORT");
}
