"use client";

import { useEffect, useState, useMemo } from "react";
import { market, TradeRecord } from "@/lib/api";
import { formatPrice, formatAmount } from "@/lib/utils";

const PAIR_BASE_PRICES: Record<string, number> = {
  "BTC/USDT": 64820, "ETH/USDT": 3180, "SOL/USDT": 148.2, "BNB/USDT": 592.1,
  "XRP/USDT": 0.5234, "DOGE/USDT": 0.132, "ADA/USDT": 0.4521,
  "AVAX/USDT": 36.42, "LINK/USDT": 17.85,
};

function mulberry32(seed: number) {
  let s = seed | 0;
  return () => { s = (s + 0x6D2B79F5) | 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

function generateMockTrades(pair: string): TradeRecord[] {
  const basePrice = PAIR_BASE_PRICES[pair] ?? 100;
  const rand = mulberry32(pair.length * 7919);
  const decimals = basePrice >= 1000 ? 2 : basePrice >= 1 ? 2 : 4;
  const now = Date.now();
  return Array.from({ length: 20 }, (_, i) => {
    const side = rand() > 0.5 ? "buy" as const : "sell" as const;
    const px = basePrice + (rand() - 0.5) * basePrice * 0.002;
    const qty = 0.001 + rand() * 1.5;
    return {
      id: `mock-${i}`,
      pair,
      price: px.toFixed(decimals),
      quantity: qty.toFixed(4),
      side,
      executed_at: new Date(now - i * 4200).toISOString(),
    };
  });
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function RecentTrades({ pair }: { pair: string }) {
  const [trades, setTrades] = useState<TradeRecord[]>([]);
  const mockTrades = useMemo(() => generateMockTrades(pair), [pair]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const data = await market.trades(pair, 30);
        if (!cancelled && data.length > 0) setTrades(data);
      } catch { /* backend offline — keep showing mock data */ }
    }
    load();
    const id = setInterval(load, 3000);
    return () => { cancelled = true; clearInterval(id); };
  }, [pair]);

  const displayTrades = trades.length > 0 ? trades : mockTrades;

  return (
    <div className="flex flex-col h-full bg-[#0d1014] overflow-hidden">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-[#1c2127] shrink-0">
        <span className="text-sm font-semibold text-[#eaecef]">Recent Trades</span>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-3 text-[10.5px] text-[#5e6673] px-3.5 py-1.5 border-b border-[#1c2127]/50 shrink-0">
        <span>Price (USDT)</span>
        <span className="text-right">Amount ({pair.split("/")[0]})</span>
        <span className="text-right">Time</span>
      </div>

      {/* Trades list */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {displayTrades.length === 0 && (
          <p className="text-xs text-[#5e6673] px-3.5 py-3">No trades yet</p>
        )}
        {displayTrades.map((t) => (
          <div
            key={t.id}
            className="grid grid-cols-3 py-[3px] px-3.5 hover:bg-white/[0.03]"
          >
            <span className={`text-[11.5px] font-mono flex items-center gap-0.5 ${t.side === "buy" ? "text-[#2ebd85]" : "text-[#f6465d]"}`}>
              {t.side === "buy" ? "▲" : "▼"}
              {formatPrice(t.price)}
            </span>
            <span className="text-right text-[11.5px] font-mono text-[#aeb6c0]">
              {formatAmount(t.quantity)}
            </span>
            <span className="text-right text-[11.5px] font-mono text-[#5e6673]">
              {formatTime(t.executed_at)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
