-- Stage B: synchronization bookkeeping.
--
-- account_transactions records which owned account a chain transaction belongs
-- to, even when it produced no fungible leg (an approval that only paid a fee,
-- an NFT-only movement, a failed call). Activity lists are driven from it.

CREATE TABLE account_transactions (
    account_id     TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    transaction_id TEXT NOT NULL REFERENCES chain_transactions(id) ON DELETE CASCADE,
    operation      TEXT NOT NULL,
    provider       TEXT NOT NULL,
    decoding       TEXT NOT NULL CHECK (decoding IN ('interpreted', 'partial', 'raw_only')),
    first_seen_at  INTEGER NOT NULL,
    PRIMARY KEY (account_id, transaction_id)
) STRICT;
CREATE INDEX account_tx_tx ON account_transactions(transaction_id);

INSERT INTO account_transactions (account_id, transaction_id, operation, provider, decoding, first_seen_at)
SELECT l.account_id, l.transaction_id, MIN(l.leg_type), t.source_provider, 'interpreted', t.occurred_at
FROM activity_legs l JOIN chain_transactions t ON t.id = l.transaction_id
GROUP BY l.account_id, l.transaction_id;
