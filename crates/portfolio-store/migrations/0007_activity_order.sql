-- Global keyset order: recent pages across many accounts must not sort all history.
CREATE INDEX chain_tx_global_order ON chain_transactions(occurred_at DESC, id DESC);
