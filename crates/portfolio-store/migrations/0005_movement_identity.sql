-- Never recycle an economic movement ID, including after disappearance/reorg.
CREATE TABLE movement_slots (
    prefix TEXT PRIMARY KEY NOT NULL,
    next_index INTEGER NOT NULL CHECK (next_index >= 0)
) STRICT;
