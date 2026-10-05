-- Stage D: remember tokens a price source had no quote for.
--
-- Wallets on the newly supported networks often hold hundreds of airdropped
-- tokens no market prices. Asking for each of them on every price refresh
-- would spend the request budget on known misses, so a miss is remembered and
-- retried after a day. Native assets are never skipped.

CREATE TABLE quote_misses (
    asset_id          TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL,
    provider_asset_id TEXT NOT NULL,
    missed_at         INTEGER NOT NULL,
    PRIMARY KEY (asset_id, provider)
) STRICT;
