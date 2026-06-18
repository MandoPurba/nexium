"use client";

import { useState, useMemo } from "react";

/* ---------- Seeded PRNG (mulberry32) for reproducible candles ---------- */
function mulberry32(seed: number) {
  return function () {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

interface Candle {
  open: number;
  high: number;
  low: number;
  close: number;
}

function generateCandles(basePrice: number, count: number, seed: number): Candle[] {
  const rng = mulberry32(seed);
  const candles: Candle[] = [];
  let price = basePrice;

  for (let i = 0; i < count; i++) {
    const volatility = price * 0.008;
    const change = (rng() - 0.48) * volatility * 2;
    const open = price;
    const close = open + change;
    const wickUp = Math.abs(change) * (0.3 + rng() * 1.2);
    const wickDown = Math.abs(change) * (0.3 + rng() * 1.2);
    const high = Math.max(open, close) + wickUp;
    const low = Math.min(open, close) - wickDown;

    candles.push({ open, high, low, close });
    price = close;
  }

  return candles;
}

const TIMEFRAMES = ["1m", "5m", "15m", "1H", "4H", "1D"] as const;

/* ---------- Per-pair config for mock generation ---------- */
const PAIR_CONFIG: Record<string, { basePrice: number; seed: number; decimals: number }> = {
  "BTC/USDT": { basePrice: 64200, seed: 42, decimals: 2 },
  "ETH/USDT": { basePrice: 3140, seed: 101, decimals: 2 },
  "SOL/USDT": { basePrice: 145, seed: 202, decimals: 2 },
  "BNB/USDT": { basePrice: 590, seed: 303, decimals: 2 },
  "XRP/USDT": { basePrice: 0.52, seed: 404, decimals: 4 },
  "DOGE/USDT": { basePrice: 0.13, seed: 505, decimals: 4 },
  "ADA/USDT": { basePrice: 0.45, seed: 606, decimals: 4 },
  "AVAX/USDT": { basePrice: 36, seed: 707, decimals: 2 },
  "LINK/USDT": { basePrice: 17.5, seed: 808, decimals: 2 },
};

function fmtPrice(n: number, decimals: number): string {
  return n.toLocaleString("en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

export function ChartPanel({ pair }: { pair: string }) {
  const [activeTimeframe, setActiveTimeframe] = useState<string>("1H");
  const [hoveredCandle, setHoveredCandle] = useState<number | null>(null);

  const config = PAIR_CONFIG[pair] ?? { basePrice: 100, seed: 999, decimals: 2 };
  const CANDLE_COUNT = 48;

  const candles = useMemo(
    () => generateCandles(config.basePrice, CANDLE_COUNT, config.seed),
    [config.basePrice, config.seed],
  );

  const allHigh = Math.max(...candles.map((c) => c.high));
  const allLow = Math.min(...candles.map((c) => c.low));
  const priceRange = allHigh - allLow;
  const padding = priceRange * 0.08;
  const chartMin = allLow - padding;
  const chartMax = allHigh + padding;
  const chartRange = chartMax - chartMin;

  const lastCandle = candles[candles.length - 1];
  const displayCandle = hoveredCandle !== null ? candles[hoveredCandle] : lastCandle;
  const lastPrice = lastCandle.close;

  /* Y position as percentage from top */
  function yPct(price: number): number {
    return ((chartMax - price) / chartRange) * 100;
  }

  /* Grid lines — 5 evenly spaced */
  const gridLines = useMemo(() => {
    const lines: number[] = [];
    for (let i = 0; i < 5; i++) {
      lines.push(chartMin + (chartRange * (i + 0.5)) / 5);
    }
    return lines.sort((a, b) => b - a);
  }, [chartMin, chartRange]);

  return (
    <div className="flex flex-col h-full bg-[#0b0e11] overflow-hidden">
      {/* Chart toolbar */}
      <div
        className="flex items-center justify-between px-3 border-b border-[#1c2127] shrink-0"
        style={{ height: 38 }}
      >
        {/* Timeframe buttons */}
        <div className="flex items-center gap-0.5">
          {TIMEFRAMES.map((tf) => (
            <button
              key={tf}
              type="button"
              onClick={() => setActiveTimeframe(tf)}
              className={`px-2.5 py-1 text-[11px] font-medium rounded transition-colors ${
                activeTimeframe === tf
                  ? "bg-[#1a2520] text-[#2ebd85]"
                  : "text-[#848e9c] hover:text-[#eaecef]"
              }`}
            >
              {tf}
            </button>
          ))}
        </div>

        {/* OHLC values */}
        <div className="flex items-center gap-3 text-[10.5px]">
          <span className="text-[#5e6673]">
            O{" "}
            <span className="font-mono text-[#eaecef]">
              {fmtPrice(displayCandle.open, config.decimals)}
            </span>
          </span>
          <span className="text-[#5e6673]">
            H{" "}
            <span className="font-mono text-[#eaecef]">
              {fmtPrice(displayCandle.high, config.decimals)}
            </span>
          </span>
          <span className="text-[#5e6673]">
            L{" "}
            <span className="font-mono text-[#eaecef]">
              {fmtPrice(displayCandle.low, config.decimals)}
            </span>
          </span>
          <span className="text-[#5e6673]">
            C{" "}
            <span
              className={`font-mono ${
                displayCandle.close >= displayCandle.open
                  ? "text-[#2ebd85]"
                  : "text-[#f6465d]"
              }`}
            >
              {fmtPrice(displayCandle.close, config.decimals)}
            </span>
          </span>
        </div>
      </div>

      {/* Chart area */}
      <div className="flex-1 relative min-h-0 overflow-hidden">
        {/* Price axis labels (right side) */}
        <div className="absolute top-0 right-0 bottom-0 w-[72px] z-10 pointer-events-none">
          {gridLines.map((price, i) => (
            <div
              key={i}
              className="absolute right-0 text-[10px] font-mono text-[#5e6673] pr-2 -translate-y-1/2"
              style={{ top: `${yPct(price)}%` }}
            >
              {fmtPrice(price, config.decimals)}
            </div>
          ))}
        </div>

        {/* Grid lines */}
        {gridLines.map((price, i) => (
          <div
            key={i}
            className="absolute left-0 right-[72px] h-px bg-[#1c2127]/60"
            style={{ top: `${yPct(price)}%` }}
          />
        ))}

        {/* Last price dashed line */}
        <div
          className="absolute left-0 right-0 h-px z-20 pointer-events-none"
          style={{
            top: `${yPct(lastPrice)}%`,
            backgroundImage:
              "repeating-linear-gradient(to right, #2ebd85 0, #2ebd85 4px, transparent 4px, transparent 8px)",
          }}
        />
        {/* Last price tag */}
        <div
          className="absolute right-0 z-20 px-1.5 py-0.5 text-[10px] font-mono text-[#0b0e11] bg-[#2ebd85] rounded-sm pointer-events-none -translate-y-1/2"
          style={{ top: `${yPct(lastPrice)}%` }}
        >
          {fmtPrice(lastPrice, config.decimals)}
        </div>

        {/* Candles */}
        <div className="absolute inset-0 right-[72px] flex items-stretch">
          {candles.map((c, i) => {
            const isGreen = c.close >= c.open;
            const color = isGreen ? "#2ebd85" : "#f6465d";

            const bodyTop = yPct(Math.max(c.open, c.close));
            const bodyBottom = yPct(Math.min(c.open, c.close));
            const bodyHeight = Math.max(bodyBottom - bodyTop, 0.3);

            const wickTop = yPct(c.high);
            const wickBottom = yPct(c.low);
            const wickHeight = wickBottom - wickTop;

            const candleWidth = 100 / CANDLE_COUNT;
            const leftPct = i * candleWidth;

            return (
              <div
                key={i}
                className="absolute"
                style={{
                  left: `${leftPct}%`,
                  width: `${candleWidth}%`,
                  top: 0,
                  bottom: 0,
                }}
                onMouseEnter={() => setHoveredCandle(i)}
                onMouseLeave={() => setHoveredCandle(null)}
              >
                {/* Wick */}
                <div
                  className="absolute left-1/2 -translate-x-1/2"
                  style={{
                    top: `${wickTop}%`,
                    height: `${wickHeight}%`,
                    width: 1,
                    backgroundColor: color,
                  }}
                />
                {/* Body */}
                <div
                  className="absolute left-[20%] right-[20%] rounded-[0.5px]"
                  style={{
                    top: `${bodyTop}%`,
                    height: `${bodyHeight}%`,
                    minHeight: 1,
                    backgroundColor: color,
                  }}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
