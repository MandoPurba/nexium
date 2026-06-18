"use client";

import { useState, useMemo } from "react";

interface MarketPair {
  base: string;
  quote: string;
  price: string;
  change: number;
  volume: string;
}

const MOCK_PAIRS: MarketPair[] = [
  { base: "BTC", quote: "USDT", price: "64,820.50", change: 2.34, volume: "18,432.5" },
  { base: "ETH", quote: "USDT", price: "3,180.42", change: 1.12, volume: "142,891.3" },
  { base: "SOL", quote: "USDT", price: "148.20", change: 5.67, volume: "2,341,020" },
  { base: "BNB", quote: "USDT", price: "592.10", change: -0.84, volume: "89,210.4" },
  { base: "XRP", quote: "USDT", price: "0.5234", change: 0.45, volume: "312,450,100" },
  { base: "DOGE", quote: "USDT", price: "0.1320", change: 3.1, volume: "1,892,301,000" },
  { base: "ADA", quote: "USDT", price: "0.4521", change: -1.2, volume: "520,120,300" },
  { base: "AVAX", quote: "USDT", price: "36.42", change: -2.05, volume: "3,401,200" },
  { base: "LINK", quote: "USDT", price: "17.85", change: 0.95, volume: "8,920,100" },
];

function formatVolume(vol: string): string {
  const n = parseFloat(vol.replace(/,/g, ""));
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(2) + "K";
  return vol;
}

export function MarketSidebar({
  activePair,
  onSelectPair,
}: {
  activePair: string;
  onSelectPair: (pair: string) => void;
}) {
  const [search, setSearch] = useState("");

  const filtered = useMemo(() => {
    if (!search.trim()) return MOCK_PAIRS;
    const q = search.trim().toLowerCase();
    return MOCK_PAIRS.filter(
      (p) =>
        p.base.toLowerCase().includes(q) ||
        `${p.base}/${p.quote}`.toLowerCase().includes(q),
    );
  }, [search]);

  return (
    <div className="flex flex-col h-full bg-[#0d1014] overflow-hidden">
      {/* Search bar */}
      <div className="px-3 py-2.5 border-b border-[#1c2127] shrink-0">
        <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded h-[32px] px-2.5 gap-2">
          {/* Magnifying glass icon */}
          <svg
            className="w-3.5 h-3.5 text-[#5e6673] shrink-0"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M21 21l-4.35-4.35M11 19a8 8 0 100-16 8 8 0 000 16z"
            />
          </svg>
          <input
            type="text"
            placeholder="Search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="flex-1 bg-transparent text-xs text-[#eaecef] placeholder:text-[#5e6673] focus:outline-none"
          />
          <span className="text-[10px] text-[#848e9c] font-medium shrink-0">
            USDT
          </span>
        </div>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-3 text-[10.5px] text-[#5e6673] px-3 py-1.5 border-b border-[#1c2127]/50 shrink-0">
        <span>Pair</span>
        <span className="text-right">Price</span>
        <span className="text-right">Change</span>
      </div>

      {/* Pair list */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {filtered.map((p) => {
          const pairStr = `${p.base}/${p.quote}`;
          const isActive = pairStr === activePair;
          const isPositive = p.change >= 0;

          return (
            <button
              key={pairStr}
              type="button"
              onClick={() => onSelectPair(pairStr)}
              className={`w-full grid grid-cols-3 items-center px-3 py-2 text-left transition-colors cursor-pointer ${
                isActive
                  ? "bg-[#2ebd85]/[0.06] border-l-2 border-l-[#2ebd85]"
                  : "border-l-2 border-l-transparent hover:bg-white/[0.03]"
              }`}
            >
              {/* Pair name + volume */}
              <div className="flex flex-col min-w-0">
                <span className="text-[12px] text-[#eaecef] truncate">
                  <span className="font-semibold">{p.base}</span>
                  <span className="text-[#5e6673]">/{p.quote}</span>
                </span>
                <span className="text-[10px] text-[#5e6673]">
                  Vol {formatVolume(p.volume)}
                </span>
              </div>

              {/* Price */}
              <span className="text-right text-[11.5px] font-mono text-[#eaecef]">
                {p.price}
              </span>

              {/* Change */}
              <span
                className={`text-right text-[11.5px] font-mono font-medium ${
                  isPositive ? "text-[#2ebd85]" : "text-[#f6465d]"
                }`}
              >
                {isPositive ? "+" : ""}
                {p.change.toFixed(2)}%
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
