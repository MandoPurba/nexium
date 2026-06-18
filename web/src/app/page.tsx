"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { Header } from "@/components/header";
import { trading, TradingPair } from "@/lib/api";

const CURRENCY_META: Record<string, { icon: string; color: string; bg: string; name: string }> = {
  BTC:  { icon: "₿", color: "#f7931a", bg: "#f7931a22", name: "Bitcoin" },
  ETH:  { icon: "Ξ", color: "#8aa0f0", bg: "#627eea22", name: "Ethereum" },
  SOL:  { icon: "◎", color: "#3fe0a8", bg: "#14f19522", name: "Solana" },
  BNB:  { icon: "B", color: "#f0b90b", bg: "#f0b90b22", name: "BNB" },
  XRP:  { icon: "X", color: "#23a4d0", bg: "#23a4d022", name: "Ripple" },
  ADA:  { icon: "₳", color: "#3cc8c8", bg: "#3cc8c822", name: "Cardano" },
  DOGE: { icon: "Ð", color: "#c2a633", bg: "#c2a63322", name: "Dogecoin" },
  DOT:  { icon: "●", color: "#e6007a", bg: "#e6007a22", name: "Polkadot" },
  AVAX: { icon: "A", color: "#e84142", bg: "#e8414222", name: "Avalanche" },
  LINK: { icon: "⬡", color: "#2a5ada", bg: "#2a5ada22", name: "Chainlink" },
};

const MOCK_MARKET: Record<string, { price: number; change: number; volume: string; high: number; low: number }> = {
  BTC:  { price: 64820,  change: 2.34,  volume: "$1.24B", high: 65210, low: 63490 },
  ETH:  { price: 3180,   change: 1.12,  volume: "$880M",  high: 3215,  low: 3102 },
  SOL:  { price: 148,    change: 5.67,  volume: "$540M",  high: 152.4, low: 139.8 },
  BNB:  { price: 592,    change: -0.84, volume: "$410M",  high: 598,   low: 584 },
  XRP:  { price: 0.52,   change: 0.45,  volume: "$320M",  high: 0.534, low: 0.511 },
  DOGE: { price: 0.13,   change: 3.10,  volume: "$260M",  high: 0.136, low: 0.124 },
  ADA:  { price: 0.45,   change: -1.20, volume: "$190M",  high: 0.462, low: 0.441 },
  AVAX: { price: 36.4,   change: -2.05, volume: "$150M",  high: 37.8,  low: 35.6 },
  LINK: { price: 17.85,  change: 0.95,  volume: "$120M",  high: 18.2,  low: 17.4 },
};

function getMeta(base: string) {
  return CURRENCY_META[base] ?? { icon: base[0], color: "#aeb6c0", bg: "#ffffff10", name: base };
}

function getMarket(base: string) {
  return MOCK_MARKET[base] ?? { price: 1.0, change: 0.0, volume: "$0", high: 1.01, low: 0.99 };
}

function formatPrice(price: number): string {
  if (price >= 1000) return price.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  if (price >= 1) return price.toFixed(2);
  return price.toFixed(price < 0.01 ? 6 : 4);
}

/** Seeded pseudo-random number generator for reproducible sparklines */
function seededRandom(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 16807 + 0) % 2147483647;
    return (s - 1) / 2147483646;
  };
}

function generateSparkline(seed: number, points: number, width: number, height: number): string {
  const rand = seededRandom(seed);
  const values: number[] = [];
  let v = 0.5;
  for (let i = 0; i < points; i++) {
    v += (rand() - 0.48) * 0.12;
    v = Math.max(0.05, Math.min(0.95, v));
    values.push(v);
  }
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;
  return values
    .map((val, i) => {
      const x = (i / (points - 1)) * width;
      const y = height - ((val - min) / range) * (height - 4) - 2;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
}

/** Simple hash from string to number for seeding */
function hashStr(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = ((h << 5) - h + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

const MOCK_PAIRS: TradingPair[] = Object.keys(MOCK_MARKET).map((base) => ({
  id: base,
  symbol: `${base}/USDT`,
  base_currency: base,
  quote_currency: "USDT",
  min_qty: "0.001",
  tick_size: "0.01",
  status: "active",
}));

export default function MarketsPage() {
  const [pairs, setPairs] = useState<TradingPair[]>(MOCK_PAIRS);

  useEffect(() => {
    trading.pairs().then((p) => { if (p.length > 0) setPairs(p); }).catch(() => {});
  }, []);

  const featured = pairs.slice(0, 4);

  return (
    <div className="flex flex-col h-screen bg-[#0b0e11] overflow-hidden">
      <Header />
      <div className="flex-1 overflow-y-auto min-h-0 p-7">
        {/* Header */}
        <div className="flex items-end justify-between mb-6">
          <div>
            <h1 className="text-[26px] font-bold tracking-[-0.02em] mb-1">Markets</h1>
            <p className="text-[13px] text-[#848e9c]">Real-time prices across {pairs.length} spot trading pairs</p>
          </div>
          <div className="flex gap-2">
            <div className="py-2 px-4 text-[13px] font-medium bg-[#161a1e] border border-[#242a31] rounded-lg text-[#848e9c] cursor-pointer">Favorites</div>
            <div className="py-2 px-4 text-[13px] font-semibold bg-[#1a2520] border border-[#2ebd85] rounded-lg text-[#2ebd85] cursor-pointer">Spot</div>
            <div className="py-2 px-4 text-[13px] font-medium bg-[#161a1e] border border-[#242a31] rounded-lg text-[#848e9c] cursor-pointer opacity-60">Futures</div>
          </div>
        </div>

        {/* Featured cards */}
        {featured.length > 0 && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3.5 mb-6">
            {featured.map((p) => {
              const base = p.base_currency;
              const meta = getMeta(base);
              const mkt = getMarket(base);
              const isPositive = mkt.change >= 0;
              const sparkPoints = generateSparkline(hashStr(base + "card"), 24, 200, 44);
              return (
                <Link
                  key={p.symbol}
                  href={`/trade?pair=${encodeURIComponent(p.symbol)}`}
                  className="bg-[#11151a] border border-[#1f252c] rounded-xl p-4 cursor-pointer hover:border-[#2a313a] transition-colors overflow-hidden relative"
                >
                  <div className="flex items-center gap-2.5 mb-3">
                    <div
                      className="w-[30px] h-[30px] rounded-full flex items-center justify-center text-xs font-bold shrink-0"
                      style={{ background: meta.bg, color: meta.color }}
                    >
                      {meta.icon}
                    </div>
                    <span className="text-[13px] font-semibold text-foreground">{base}</span>
                    <span className="text-[11px] text-[#5e6673]">{meta.name}</span>
                    <span className="flex-1" />
                    <span
                      className="text-[12px] font-semibold font-mono"
                      style={{ color: isPositive ? "#2ebd85" : "#f6465d" }}
                    >
                      {isPositive ? "+" : ""}{mkt.change.toFixed(2)}%
                    </span>
                  </div>
                  <div className="text-[20px] font-semibold font-mono text-foreground mb-3">
                    ${formatPrice(mkt.price)}
                  </div>
                  <svg viewBox="0 0 200 44" className="w-full h-[44px]" preserveAspectRatio="none">
                    <polyline
                      points={sparkPoints}
                      fill="none"
                      stroke={isPositive ? "#2ebd85" : "#f6465d"}
                      strokeWidth="1.5"
                      strokeLinejoin="round"
                    />
                  </svg>
                </Link>
              );
            })}
          </div>
        )}

        {/* Market table */}
        <div className="bg-[#0d1014] border border-[#1c2127] rounded-xl overflow-hidden">
          {/* Table header */}
          <div className="flex items-center py-3.5 px-5 border-b border-[#1c2127] text-[11px] text-[#5e6673] uppercase tracking-wider">
            <span className="w-7" />
            <span className="flex-[2]">Name</span>
            <span className="flex-[1.3] text-right">Last Price</span>
            <span className="flex-[1] text-right">24h Change</span>
            <span className="flex-[1] text-right">24h High</span>
            <span className="flex-[1] text-right">24h Low</span>
            <span className="flex-[1.2] text-right">24h Volume</span>
            <span className="w-[120px] text-center">Last 7 Days</span>
            <span className="w-[90px] text-right">Action</span>
          </div>
          {/* Table rows */}
          {pairs.length === 0 && (
            <div className="py-8 text-center text-sm text-[#5e6673]">No pairs available</div>
          )}
          {pairs.map((p) => {
            const base = p.base_currency;
            const meta = getMeta(base);
            const mkt = getMarket(base);
            const isPositive = mkt.change >= 0;
            const sparkPoints = generateSparkline(hashStr(base + "7d"), 30, 110, 30);
            return (
              <div key={p.symbol} className="flex items-center py-3.5 px-5 border-b border-[#14181d] hover:bg-white/[0.02] transition-colors">
                <span className="w-7 text-[#3a4148]">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 2l2.9 6.3 6.9.7-5.2 4.6 1.5 6.8L12 17.8 5.9 20.4l1.5-6.8L2.2 9l6.9-.7z" /></svg>
                </span>
                <span className="flex-[2] flex items-center gap-3">
                  <span
                    className="w-[30px] h-[30px] rounded-full flex items-center justify-center text-xs font-bold shrink-0"
                    style={{ background: meta.bg, color: meta.color }}
                  >
                    {meta.icon}
                  </span>
                  <span className="flex flex-col">
                    <span className="text-sm font-semibold">{p.symbol}</span>
                    <span className="text-[11px] text-[#5e6673]">{meta.name}</span>
                  </span>
                </span>
                <span className="flex-[1.3] text-right text-[14px] font-mono text-foreground">
                  ${formatPrice(mkt.price)}
                </span>
                <span
                  className="flex-[1] text-right text-[13px] font-bold font-mono"
                  style={{ color: isPositive ? "#2ebd85" : "#f6465d" }}
                >
                  {isPositive ? "+" : ""}{mkt.change.toFixed(2)}%
                </span>
                <span className="flex-[1] text-right text-[13px] font-mono text-[#aeb6c0]">
                  ${formatPrice(mkt.high)}
                </span>
                <span className="flex-[1] text-right text-[13px] font-mono text-[#aeb6c0]">
                  ${formatPrice(mkt.low)}
                </span>
                <span className="flex-[1.2] text-right text-[13px] font-mono text-[#aeb6c0]">
                  {mkt.volume}
                </span>
                <span className="w-[120px] flex justify-center">
                  <svg viewBox="0 0 110 30" width="110" height="30">
                    <polyline
                      points={sparkPoints}
                      fill="none"
                      stroke={isPositive ? "#2ebd85" : "#f6465d"}
                      strokeWidth="1.5"
                      strokeLinejoin="round"
                    />
                  </svg>
                </span>
                <span className="w-[90px] text-right">
                  <Link
                    href={`/trade?pair=${encodeURIComponent(p.symbol)}`}
                    className="inline-block text-[12px] font-semibold text-[#eaecef] bg-[#161a1e] border border-[#2a313a] rounded-[7px] hover:bg-[#1c2127] transition-colors"
                    style={{ padding: "7px 16px" }}
                  >
                    Trade
                  </Link>
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
