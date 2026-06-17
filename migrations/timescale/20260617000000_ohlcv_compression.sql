-- Compression policy for market.ohlcv hypertable.
-- Chunks older than 7 days are compressed automatically.

ALTER TABLE market.ohlcv SET (
    timescaledb.compress,
    timescaledb.compress_segmentby = 'pair, interval',
    timescaledb.compress_orderby = 'bucket DESC'
);

SELECT add_compression_policy('market.ohlcv', INTERVAL '7 days', if_not_exists => TRUE);
