-- Match scope_valuation_lots' exact textual grouping. A covering partial index
-- avoids a temporary GROUP BY sort over all open FIFO lots during startup.
-- Detailed FIFO order still uses lots_account_asset; calculations are unchanged.
CREATE INDEX lots_valuation_remaining
ON lots(account_id, asset_id, remaining_quantity, remaining_basis_usd, basis_kind)
WHERE remaining_quantity != '0';
