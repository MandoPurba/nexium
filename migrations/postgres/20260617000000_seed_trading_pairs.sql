-- Seed initial trading pairs for Sprint 4.
-- min_qty and tick_size are intentionally small to allow test orders.
INSERT INTO trading.pairs (symbol, base_currency, quote_currency, min_qty, tick_size)
VALUES
    ('BTC/USDT', 'BTC', 'USDT', 0.0001, 0.01),
    ('ETH/USDT', 'ETH', 'USDT', 0.001,  0.01)
ON CONFLICT (symbol) DO NOTHING;
