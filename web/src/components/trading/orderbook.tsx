"use client";

import { useEffect, useState, useMemo } from "react";
import { market, OrderBookSnapshot } from "@/lib/api";
import { formatPrice, formatAmount } from "@/lib/utils";

function computeDepth(levels: [string, string][]): {
  price: string;
  qty: string;
  total: number;
  depthPct: number;
}[] {
  let cumulative = 0;
  const rows = levels.map(([price, qty]) => {
    const q = parseFloat(qty);
    cumulative += q;
    return { price, qty, total: cumulative };
  });
  const maxTotal = rows[rows.length - 1]?.total ?? 1;
  return rows.map((r) => ({ ...r, depthPct: (r.total / maxTotal) * 100 }));
}

const PAIR_BASE_PRICES: Record<string, number> = {
  "BTC/USDT": 64820, "ETH/USDT": 3180, "SOL/USDT": 148.2, "BNB/USDT": 592.1,
  "XRP/USDT": 0.5234, "DOGE/USDT": 0.132, "ADA/USDT": 0.4521,
  "AVAX/USDT": 36.42, "LINK/USDT": 17.85,
};

function mulberry32(seed: number) {
  let s = seed | 0;
  return () => { s = (s + 0x6D2B79F5) | 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

function generateMockBook(pair: string): OrderBookSnapshot {
  const basePrice = PAIR_BASE_PRICES[pair] ?? 100;
  const rand = mulberry32(pair.length * 9973);
  const decimals = basePrice >= 1000 ? 2 : basePrice >= 1 ? 2 : 4;
  const asks: [string, string][] = [];
  const bids: [string, string][] = [];
  const spread = basePrice * 0.0003;
  for (let i = 0; i < 12; i++) {
    const askP = basePrice + spread / 2 + (i * basePrice * 0.0004) + rand() * basePrice * 0.0002;
    const bidP = basePrice - spread / 2 - (i * basePrice * 0.0004) - rand() * basePrice * 0.0002;
    const askQ = 0.01 + rand() * 2;
    const bidQ = 0.01 + rand() * 2;
    asks.push([askP.toFixed(decimals), askQ.toFixed(4)]);
    bids.push([bidP.toFixed(decimals), bidQ.toFixed(4)]);
  }
  return { pair, asks, bids, timestamp: new Date().toISOString() };
}

export function Orderbook({ pair }: { pair: string }) {
  const [book, setBook] = useState<OrderBookSnapshot | null>(null);
  const [pulse, setPulse] = useState(false);
  const [apiDown, setApiDown] = useState(false);

  const mockBook = useMemo(() => generateMockBook(pair), [pair]);

  useEffect(() => {
    let cancelled = false;
    let failCount = 0;
    async function load() {
      try {
        const data = await market.orderbook(pair);
        if (!cancelled) {
          setBook(data);
          setApiDown(false);
          setPulse(true);
          setTimeout(() => setPulse(false), 400);
          failCount = 0;
        }
      } catch {
        failCount++;
        if (failCount >= 2 && !cancelled) setApiDown(true);
      }
    }
    load();
    const id = setInterval(load, 2000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [pair]);

  const activeBook = book ?? (apiDown ? mockBook : null);

  // Asks: ascending, show lowest 10, display reversed (lowest near spread)
  const rawAsks = activeBook
    ? [...activeBook.asks].sort((a, b) => parseFloat(a[0]) - parseFloat(b[0])).slice(0, 10)
    : [];
  const rawBids = activeBook
    ? [...activeBook.bids].sort((a, b) => parseFloat(b[0]) - parseFloat(a[0])).slice(0, 10)
    : [];

  const askDepth = computeDepth([...rawAsks].reverse());
  const bidDepth = computeDepth(rawBids);

  // asks display: reversed so lowest ask is nearest to spread
  const asksDisplay = rawAsks
    .map((lvl, i) => {
      const depthRow = askDepth[rawAsks.length - 1 - i];
      return {
        price: lvl[0],
        qty: lvl[1],
        total: depthRow?.total ?? 0,
        depthPct: depthRow?.depthPct ?? 0,
      };
    })
    .reverse();

  const bidsDisplay = bidDepth;

  const bestAsk = rawAsks[0] ? parseFloat(rawAsks[0][0]) : null;
  const bestBid = rawBids[0] ? parseFloat(rawBids[0][0]) : null;
  const spread = bestAsk !== null && bestBid !== null ? bestAsk - bestBid : null;
  const spreadPct =
    spread !== null && bestBid !== null && bestBid > 0
      ? ((spread / bestBid) * 100).toFixed(3)
      : null;
  const midPrice =
    bestAsk !== null && bestBid !== null ? (bestAsk + bestBid) / 2 : null;

  return (
    <div className="flex flex-col h-full bg-[#0d1014] overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between px-3.5 py-2.5 border-b border-[#1c2127] shrink-0">
        <span className="text-sm font-semibold text-foreground">Order Book</span>
        <div
          className={`w-1.5 h-1.5 rounded-full transition-colors duration-300 ${
            pulse ? "bg-[#2ebd85]" : "bg-[#2ebd85]/30"
          }`}
          title="Live"
        />
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-3 text-[10.5px] text-[#5e6673] px-3.5 py-1.5 border-b border-[#1c2127]/50 shrink-0">
        <span>Price (USDT)</span>
        <span className="text-right">Amount ({pair.split("/")[0]})</span>
        <span className="text-right">Total</span>
      </div>

      {!activeBook ? (
        <div className="flex-1 flex items-center justify-center">
          <div className="flex flex-col items-center gap-2">
            <div className="w-5 h-5 border border-[#2ebd85] border-t-transparent rounded-full animate-spin" />
            <span className="text-xs text-[#5e6673]">Loading…</span>
          </div>
        </div>
      ) : (
        <div className="flex flex-col flex-1 overflow-hidden">
          {/* Asks — flex-col-reverse so lowest ask is bottom (near spread) */}
          <div className="flex-1 flex flex-col justify-end overflow-hidden">
            {asksDisplay.map((row, i) => (
              <div
                key={`a-${i}`}
                className="relative grid grid-cols-3 py-[3px] px-3.5 hover:bg-white/[0.03] cursor-pointer group"
              >
                <div
                  className="absolute inset-y-0 right-0 bg-[#f6465d]/10 transition-all"
                  style={{ width: `${row.depthPct}%` }}
                />
                <span className="text-[11.5px] font-mono text-[#f6465d] relative z-10">
                  {formatPrice(row.price)}
                </span>
                <span className="text-right text-[11.5px] font-mono text-[#aeb6c0] relative z-10">
                  {formatAmount(row.qty)}
                </span>
                <span className="text-right text-[11.5px] font-mono text-[#5e6673] relative z-10">
                  {formatAmount(String(row.total))}
                </span>
              </div>
            ))}
          </div>

          {/* Spread / Mid price row */}
          <div className="flex items-center justify-between px-3.5 py-2 border-y border-[#1c2127]/50 bg-[#0b0e11] shrink-0">
            {midPrice !== null ? (
              <>
                <span className="text-base font-semibold font-mono text-foreground">
                  {formatPrice(String(midPrice))}
                </span>
                {spread !== null && spreadPct !== null && (
                  <span className="text-[10.5px] text-[#5e6673]">
                    Spread {formatPrice(String(spread))} ({spreadPct}%)
                  </span>
                )}
              </>
            ) : (
              <span className="text-xs text-[#5e6673]">Spread —</span>
            )}
          </div>

          {/* Bids */}
          <div className="flex-1 overflow-hidden">
            {bidsDisplay.map((row, i) => (
              <div
                key={`b-${i}`}
                className="relative grid grid-cols-3 py-[3px] px-3.5 hover:bg-white/[0.03] cursor-pointer group"
              >
                <div
                  className="absolute inset-y-0 right-0 bg-[#2ebd85]/10 transition-all"
                  style={{ width: `${row.depthPct}%` }}
                />
                <span className="text-[11.5px] font-mono text-[#2ebd85] relative z-10">
                  {formatPrice(row.price)}
                </span>
                <span className="text-right text-[11.5px] font-mono text-[#aeb6c0] relative z-10">
                  {formatAmount(row.qty)}
                </span>
                <span className="text-right text-[11.5px] font-mono text-[#5e6673] relative z-10">
                  {formatAmount(String(row.total))}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
