-- Stage C: accounting replay results, historical price coverage, CSV imports.
--
-- Everything in account_accounting, asset_accounting, accounting_flows,
-- leg_accounting, reconciliation_items, lots and lot_consumptions is derived:
-- a deterministic replay deletes and rebuilds it from chain evidence, prices
-- and the versioned user decisions in accounting_overrides.

CREATE TABLE account_accounting (
    account_id        TEXT NOT NULL,
    asset_id          TEXT NOT NULL, -- '' for the whole-account row
    realized_usd      TEXT NOT NULL,
    realized_complete INTEGER NOT NULL CHECK (realized_complete IN (0, 1)),
    income_usd        TEXT NOT NULL,
    income_complete   INTEGER NOT NULL CHECK (income_complete IN (0, 1)),
    expense_usd       TEXT NOT NULL,
    expense_complete  INTEGER NOT NULL CHECK (expense_complete IN (0, 1)),
    fee_charges       INTEGER NOT NULL,
    PRIMARY KEY (account_id, asset_id)
) STRICT;

-- Capital movements that may cross a scope boundary (Modified Dietz flows).
-- A flow counts for a scope when exactly one of its endpoints is inside it.
CREATE TABLE accounting_flows (
    event_id        TEXT PRIMARY KEY NOT NULL,
    at              INTEGER NOT NULL,
    from_account_id TEXT,
    to_account_id   TEXT,
    asset_id        TEXT NOT NULL,
    quantity        TEXT NOT NULL,
    value_usd       TEXT,          -- NULL: no reliable valuation
    classified      INTEGER NOT NULL CHECK (classified IN (0, 1))
) STRICT;
CREATE INDEX accounting_flows_at ON accounting_flows(at);

-- How the replay interpreted each leg and fee, for activity and review lists.
CREATE TABLE leg_accounting (
    leg_id                  TEXT PRIMARY KEY NOT NULL, -- activity_legs.id or transaction_fees.id
    transaction_id          TEXT NOT NULL,
    account_id              TEXT NOT NULL,
    asset_id                TEXT NOT NULL,
    occurred_at             INTEGER NOT NULL,
    quantity                TEXT NOT NULL, -- signed asset units
    treatment               TEXT NOT NULL,
    counterparty_account_id TEXT,
    value_usd               TEXT,
    value_estimated         INTEGER NOT NULL DEFAULT 0 CHECK (value_estimated IN (0, 1)),
    basis_usd               TEXT,
    basis_kind              TEXT,
    proceeds_usd            TEXT,
    review                  TEXT -- NULL, or why the user should look at it
) STRICT;
CREATE INDEX leg_accounting_review ON leg_accounting(review) WHERE review IS NOT NULL;
CREATE INDEX leg_accounting_tx ON leg_accounting(transaction_id, account_id);
CREATE INDEX leg_accounting_asset ON leg_accounting(asset_id, occurred_at);

CREATE TABLE reconciliation_items (
    id         TEXT PRIMARY KEY NOT NULL,
    kind       TEXT NOT NULL,
    account_id TEXT,
    asset_id   TEXT,
    event_id   TEXT,
    quantity   TEXT,
    detail     TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX lot_consumptions_lot ON lot_consumptions(lot_id);
CREATE INDEX lots_parent ON lots(parent_lot_id);

-- Which daily price history has been downloaded for an asset.
CREATE TABLE price_history_coverage (
    asset_id          TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL,
    provider_asset_id TEXT NOT NULL,
    covered_from      INTEGER,
    covered_to        INTEGER,
    retry_after       INTEGER, -- a missing series is not re-requested before this time
    last_error        TEXT,
    updated_at        INTEGER NOT NULL,
    PRIMARY KEY (asset_id, provider)
) STRICT;

CREATE INDEX overrides_target ON accounting_overrides(target_kind, target_id, version);

ALTER TABLE import_batches ADD COLUMN file_name TEXT NOT NULL DEFAULT '';
ALTER TABLE import_batches ADD COLUMN rows_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE import_batches ADD COLUMN committed_at INTEGER;

INSERT INTO app_meta (key, value) VALUES ('accounting_dirty', '1')
ON CONFLICT(key) DO UPDATE SET value = '1';
