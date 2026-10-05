-- Portfolio Desk initial schema.
-- Quantities, prices and USD amounts are exact decimal TEXT; never REAL.
-- Timestamps are Unix seconds (UTC) unless the column name says otherwise.

CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE wallets (
    id         TEXT PRIMARY KEY NOT NULL,
    label      TEXT NOT NULL CHECK (length(trim(label)) > 0),
    archived   INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE accounts (
    id                TEXT PRIMARY KEY NOT NULL,
    wallet_id         TEXT NOT NULL REFERENCES wallets(id) ON DELETE RESTRICT,
    network_id        TEXT NOT NULL,
    canonical_address TEXT NOT NULL,
    display_address   TEXT NOT NULL,
    label             TEXT,
    archived          INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    created_at        INTEGER NOT NULL,
    UNIQUE (network_id, canonical_address)
) STRICT;
CREATE INDEX accounts_wallet ON accounts(wallet_id);

CREATE TABLE groups (
    id         TEXT PRIMARY KEY NOT NULL,
    label      TEXT NOT NULL CHECK (length(trim(label)) > 0),
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE group_wallets (
    group_id  TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    wallet_id TEXT NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, wallet_id)
) STRICT;

CREATE TABLE assets (
    id                   TEXT PRIMARY KEY NOT NULL,
    network_id           TEXT NOT NULL,
    asset_kind           TEXT NOT NULL CHECK (asset_kind IN ('native', 'token')),
    canonical_identifier TEXT NOT NULL, -- '' for native; contract/mint/master otherwise
    decimals             INTEGER NOT NULL CHECK (decimals BETWEEN 0 AND 255),
    symbol               TEXT,
    name                 TEXT,
    verification         TEXT NOT NULL DEFAULT 'unverified'
                         CHECK (verification IN ('verified', 'unverified', 'spam')),
    metadata_provider    TEXT,
    metadata_updated_at  INTEGER,
    UNIQUE (network_id, asset_kind, canonical_identifier)
) STRICT;

CREATE TABLE asset_mappings (
    id                TEXT PRIMARY KEY NOT NULL,
    asset_id          TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL,
    provider_asset_id TEXT NOT NULL,
    display_group     TEXT,
    confidence        TEXT NOT NULL CHECK (confidence IN ('verified', 'high', 'low')),
    manually_verified INTEGER NOT NULL DEFAULT 0 CHECK (manually_verified IN (0, 1)),
    version           INTEGER NOT NULL DEFAULT 1,
    effective_at      INTEGER NOT NULL,
    UNIQUE (asset_id, provider, version)
) STRICT;

CREATE TABLE balance_observations (
    id           INTEGER PRIMARY KEY,
    account_id   TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    asset_id     TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    raw_quantity TEXT NOT NULL, -- exact integer in smallest units
    observed_at  INTEGER NOT NULL,
    chain_height INTEGER,
    chain_ref    TEXT,          -- block hash / slot / seqno when known
    provider     TEXT NOT NULL,
    status       TEXT NOT NULL CHECK (status IN ('fresh', 'stale', 'missing', 'conflicted'))
) STRICT;
CREATE INDEX balance_obs_account_asset_time ON balance_observations(account_id, asset_id, observed_at);

CREATE TABLE chain_transactions (
    id              TEXT PRIMARY KEY NOT NULL,
    network_id      TEXT NOT NULL,
    canonical_tx_id TEXT NOT NULL,
    block_height    INTEGER,
    position        TEXT,  -- network-specific ordering key (log index, logical time...)
    occurred_at     INTEGER NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('pending', 'confirmed', 'final', 'failed', 'reorged')),
    source_provider TEXT NOT NULL,
    source_refs     TEXT NOT NULL DEFAULT '[]', -- sanitized JSON evidence references
    UNIQUE (network_id, canonical_tx_id)
) STRICT;
CREATE INDEX chain_tx_time ON chain_transactions(network_id, occurred_at);

CREATE TABLE activity_legs (
    id                 TEXT PRIMARY KEY NOT NULL, -- stable leg identity
    transaction_id     TEXT NOT NULL REFERENCES chain_transactions(id) ON DELETE CASCADE,
    account_id         TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    asset_id           TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    signed_raw_quantity TEXT NOT NULL,
    direction          TEXT NOT NULL CHECK (direction IN ('in', 'out', 'self')),
    leg_type           TEXT NOT NULL,
    classification     TEXT,
    decoding           TEXT NOT NULL CHECK (decoding IN ('interpreted', 'partial', 'raw_only')),
    unresolved         INTEGER NOT NULL DEFAULT 0 CHECK (unresolved IN (0, 1)),
    evidence           TEXT NOT NULL DEFAULT '{}'
) STRICT;
CREATE INDEX legs_account_time ON activity_legs(account_id, transaction_id);
CREATE INDEX legs_asset ON activity_legs(asset_id);
CREATE INDEX legs_unresolved ON activity_legs(unresolved) WHERE unresolved = 1;

CREATE TABLE transaction_fees (
    id             TEXT PRIMARY KEY NOT NULL,
    transaction_id TEXT NOT NULL REFERENCES chain_transactions(id) ON DELETE CASCADE,
    payer_account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
    asset_id       TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    raw_quantity   TEXT NOT NULL,
    attribution    TEXT NOT NULL CHECK (attribution IN ('exact', 'shared', 'unknown', 'sponsored')),
    UNIQUE (transaction_id, payer_account_id, asset_id)
) STRICT;

CREATE TABLE sync_checkpoints (
    id               TEXT PRIMARY KEY NOT NULL,
    account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    provider         TEXT NOT NULL,
    category         TEXT NOT NULL,
    backfill_cursor  TEXT,
    forward_cursor   TEXT,
    boundary         TEXT,
    coverage         TEXT NOT NULL DEFAULT 'loading'
                     CHECK (coverage IN ('loading', 'paused', 'complete', 'partial', 'unsupported')),
    earliest_covered_at INTEGER,
    retry_state      TEXT NOT NULL DEFAULT '{}',
    updated_at       INTEGER NOT NULL,
    UNIQUE (account_id, provider, category)
) STRICT;

CREATE TABLE prices (
    id            INTEGER PRIMARY KEY,
    asset_id      TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    provider      TEXT NOT NULL,
    price_usd     TEXT NOT NULL,
    requested_at  INTEGER NOT NULL,
    observed_at   INTEGER NOT NULL,
    granularity   TEXT NOT NULL CHECK (granularity IN ('tick', 'minute', 'hour', 'day')),
    quality       TEXT NOT NULL CHECK (quality IN ('current', 'stale', 'manual', 'low_confidence', 'estimated')),
    change_24h_percent TEXT,
    mapping_version INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX prices_asset_time ON prices(asset_id, observed_at);

CREATE TABLE accounting_overrides (
    id           TEXT PRIMARY KEY NOT NULL,
    target_kind  TEXT NOT NULL CHECK (target_kind IN ('leg', 'lot', 'transaction', 'price')),
    target_id    TEXT NOT NULL,
    version      INTEGER NOT NULL,
    payload      TEXT NOT NULL, -- JSON; exact decimals as strings
    evidence_ref TEXT,
    orphaned     INTEGER NOT NULL DEFAULT 0 CHECK (orphaned IN (0, 1)),
    created_at   INTEGER NOT NULL,
    UNIQUE (target_kind, target_id, version)
) STRICT;

CREATE TABLE lots (
    id                TEXT PRIMARY KEY NOT NULL,
    account_id        TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    asset_id          TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    quantity          TEXT NOT NULL,
    remaining_quantity TEXT NOT NULL,
    basis_usd         TEXT,  -- NULL means unknown, distinct from '0'
    remaining_basis_usd TEXT,
    basis_kind        TEXT NOT NULL CHECK (basis_kind IN ('known', 'estimated', 'unknown')),
    acquired_at       INTEGER NOT NULL,
    arrived_at        INTEGER NOT NULL,
    parent_lot_id     TEXT REFERENCES lots(id) ON DELETE SET NULL,
    source_event      TEXT NOT NULL,
    method_version    INTEGER NOT NULL
) STRICT;
CREATE INDEX lots_account_asset ON lots(account_id, asset_id, acquired_at);

CREATE TABLE lot_consumptions (
    id            TEXT PRIMARY KEY NOT NULL,
    lot_id        TEXT NOT NULL REFERENCES lots(id) ON DELETE CASCADE,
    event_id      TEXT NOT NULL,
    quantity      TEXT NOT NULL,
    basis_usd     TEXT,
    kind          TEXT NOT NULL CHECK (kind IN ('disposal', 'fee', 'transfer', 'withdrawal')),
    method_version INTEGER NOT NULL
) STRICT;

CREATE TABLE portfolio_snapshots (
    id            INTEGER PRIMARY KEY,
    account_id    TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    asset_id      TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    at            INTEGER NOT NULL,
    raw_quantity  TEXT NOT NULL,
    quality       TEXT NOT NULL CHECK (quality IN ('observed', 'reconstructed', 'estimated')),
    valuation_version INTEGER NOT NULL,
    UNIQUE (account_id, asset_id, at)
) STRICT;

CREATE TABLE import_batches (
    id          TEXT PRIMARY KEY NOT NULL,
    file_sha256 TEXT NOT NULL,
    mapping     TEXT NOT NULL,
    row_ids     TEXT NOT NULL DEFAULT '[]',
    status      TEXT NOT NULL CHECK (status IN ('preview', 'committed', 'rolled_back')),
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE provider_usage (
    provider   TEXT NOT NULL,
    day_utc    TEXT NOT NULL, -- YYYY-MM-DD
    requests   INTEGER NOT NULL DEFAULT 0,
    credits    TEXT NOT NULL DEFAULT '0',
    last_error TEXT,
    PRIMARY KEY (provider, day_utc)
) STRICT;
