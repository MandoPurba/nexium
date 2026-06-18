"use client";

import { useState } from "react";
import { Header } from "@/components/header";
import { MarketSidebar } from "@/components/trading/market-sidebar";
import { ChartPanel } from "@/components/trading/chart-panel";
import { Orderbook } from "@/components/trading/orderbook";
import { OrderForm } from "@/components/trading/order-form";
import { RecentTrades } from "@/components/trading/recent-trades";
import { OpenOrders } from "@/components/trading/open-orders";

/* ---------- Per-pair mock ticker data ---------- */
const TICKER_DATA: Record<
  string,
  {
    name: string;
    lastPrice: string;
    change: number;
    high24h: string;
    low24h: string;
    volume24h: string;
  }
> = {
  "BTC/USDT": { name: "Bitcoin", lastPrice: "64,820.50", change: 2.34, high24h: "65,800.00", low24h: "63,210.50", volume24h: "18,432 BTC" },
  "ETH/USDT": { name: "Ethereum", lastPrice: "3,180.42", change: 1.12, high24h: "3,220.80", low24h: "3,120.10", volume24h: "142,891 ETH" },
  "SOL/USDT": { name: "Solana", lastPrice: "148.20", change: 5.67, high24h: "152.40", low24h: "139.80", volume24h: "2.34M SOL" },
  "BNB/USDT": { name: "BNB", lastPrice: "592.10", change: -0.84, high24h: "598.50", low24h: "585.20", volume24h: "89,210 BNB" },
  "XRP/USDT": { name: "Ripple", lastPrice: "0.5234", change: 0.45, high24h: "0.5310", low24h: "0.5180", volume24h: "312.4M XRP" },
  "DOGE/USDT": { name: "Dogecoin", lastPrice: "0.1320", change: 3.1, high24h: "0.1355", low24h: "0.1275", volume24h: "1.89B DOGE" },
  "ADA/USDT": { name: "Cardano", lastPrice: "0.4521", change: -1.2, high24h: "0.4620", low24h: "0.4480", volume24h: "520.1M ADA" },
  "AVAX/USDT": { name: "Avalanche", lastPrice: "36.42", change: -2.05, high24h: "37.80", low24h: "35.90", volume24h: "3.40M AVAX" },
  "LINK/USDT": { name: "Chainlink", lastPrice: "17.85", change: 0.95, high24h: "18.10", low24h: "17.50", volume24h: "8.92M LINK" },
};

const DEFAULT_PAIR = "BTC/USDT";

const ICON_META: Record<string, { icon: string; color: string; bg: string }> = {
  BTC:  { icon: "₿", color: "#f7931a", bg: "#f7931a22" },
  ETH:  { icon: "Ξ", color: "#8aa0f0", bg: "#627eea22" },
  SOL:  { icon: "◎", color: "#3fe0a8", bg: "#14f19522" },
  BNB:  { icon: "B", color: "#f0b90b", bg: "#f0b90b22" },
  XRP:  { icon: "X", color: "#23a4d0", bg: "#23a4d022" },
  DOGE: { icon: "Ð", color: "#c2a633", bg: "#c2a63322" },
  ADA:  { icon: "₳", color: "#3cc8c8", bg: "#3cc8c822" },
  AVAX: { icon: "A", color: "#e84142", bg: "#e8414222" },
  LINK: { icon: "⬡", color: "#2a5ada", bg: "#2a5ada22" },
};

/* ---------- Pair Header ---------- */
function PairHeader({ pair }: { pair: string }) {
  const base = pair.split("/")[0];
  const ticker = TICKER_DATA[pair] ?? TICKER_DATA[DEFAULT_PAIR]!;
  const isPositive = ticker.change >= 0;
  const changeColor = isPositive ? "text-[#2ebd85]" : "text-[#f6465d]";
  const meta = ICON_META[base] ?? { icon: base[0], color: "#aeb6c0", bg: "#ffffff10" };

  return (
    <div
      className="flex items-center gap-5 px-4 border-b border-[#1c2127] bg-[#0b0e11] overflow-x-auto shrink-0"
      style={{ height: 72 }}
    >
      {/* Coin icon + symbol + name */}
      <div className="flex items-center gap-2.5 shrink-0">
        <div
          className="rounded-full flex items-center justify-center text-xs font-bold"
          style={{ width: 34, height: 34, background: meta.bg, color: meta.color }}
        >
          {meta.icon}
        </div>
        <div>
          <span className="text-[15px] font-bold text-[#eaecef] tracking-tight">
            {pair}
          </span>
          <div className="text-[11px] text-[#5e6673]">{ticker.name}</div>
        </div>
      </div>

      {/* Separator */}
      <div className="w-px h-9 bg-[#1c2127] shrink-0" />

      {/* Last price */}
      <div className="shrink-0">
        <div className={`text-[22px] font-semibold font-mono leading-tight ${changeColor}`}>
          {ticker.lastPrice}
        </div>
        <div className="text-[10.5px] text-[#5e6673]">Last price</div>
      </div>

      {/* Separator */}
      <div className="w-px h-9 bg-[#1c2127] shrink-0" />

      {/* Stats */}
      <div className="flex gap-5 text-xs shrink-0">
        <div>
          <div className={`font-mono font-semibold ${changeColor}`}>
            {isPositive ? "+" : ""}
            {ticker.change.toFixed(2)}%
          </div>
          <div className="text-[#5e6673] mt-0.5 text-[10.5px]">24h Change</div>
        </div>
        <div>
          <div className="font-mono text-[#eaecef]">{ticker.high24h}</div>
          <div className="text-[#5e6673] mt-0.5 text-[10.5px]">24h High</div>
        </div>
        <div>
          <div className="font-mono text-[#eaecef]">{ticker.low24h}</div>
          <div className="text-[#5e6673] mt-0.5 text-[10.5px]">24h Low</div>
        </div>
        <div>
          <div className="font-mono text-[#eaecef]">{ticker.volume24h}</div>
          <div className="text-[#5e6673] mt-0.5 text-[10.5px]">24h Volume</div>
        </div>
      </div>
    </div>
  );
}

/* ---------- Trade Page ---------- */
export default function TradePage() {
  const [pair, setPair] = useState(DEFAULT_PAIR);
  const [refreshKey, setRefreshKey] = useState(0);

  return (
    <div className="flex flex-col h-screen bg-[#0b0e11] overflow-hidden">
      <Header />

      {/* Main 4-column layout */}
      <div className="flex flex-1 min-h-0 overflow-hidden">
        {/* Column 1: Market sidebar */}
        <div
          className="shrink-0 border-r border-[#1c2127] flex flex-col min-h-0"
          style={{ width: 248 }}
        >
          <MarketSidebar activePair={pair} onSelectPair={setPair} />
        </div>

        {/* Column 2: Center — pair header + chart */}
        <div className="flex-1 flex flex-col min-h-0 min-w-[560px] border-r border-[#1c2127]">
          <PairHeader pair={pair} />
          <div className="flex-1 min-h-0">
            <ChartPanel pair={pair} />
          </div>
        </div>

        {/* Column 3: Orderbook + Recent trades */}
        <div
          className="shrink-0 border-r border-[#1c2127] flex flex-col min-h-0"
          style={{ width: 268 }}
        >
          {/* Order Book — takes most space */}
          <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
            <Orderbook pair={pair} />
          </div>

          {/* Recent Trades — pushed toward bottom */}
          <div className="shrink-0 border-t border-[#1c2127] flex flex-col overflow-hidden" style={{ minHeight: 80, maxHeight: 240 }}>
            <RecentTrades pair={pair} />
          </div>
        </div>

        {/* Column 4: Order form */}
        <div
          className="shrink-0 flex flex-col min-h-0 overflow-y-auto"
          style={{ width: 300 }}
        >
          <div className="p-4 flex flex-col gap-[13px]">
            <OrderForm
              pair={pair}
              onOrderPlaced={() => setRefreshKey((k) => k + 1)}
            />
          </div>
          <div className="flex-1 min-h-0 overflow-y-auto">
            <OpenOrders pair={pair} refreshKey={refreshKey} />
          </div>
        </div>
      </div>
    </div>
  );
}
