-- User decisions stay independent from provider verification updates.
CREATE TABLE asset_preferences (
    asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK (hidden IN (0,1)),
    exclude_override INTEGER CHECK (exclude_override IN (0,1))
) STRICT;
