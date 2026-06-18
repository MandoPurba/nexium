-- Seed default fee tiers for the exchange.

INSERT INTO fee.fee_tiers (level, maker_rate, taker_rate, min_volume_30d)
VALUES
    ('standard', 0.001000, 0.002000, 0),
    ('vip1',     0.000800, 0.001500, 100000),
    ('vip2',     0.000500, 0.001000, 1000000)
ON CONFLICT (level) DO NOTHING;
