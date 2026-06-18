"use client";

import { useEffect, useState } from "react";
import { useAuth } from "@/lib/auth-context";
import { Header } from "@/components/header";
import { wallet, Wallet } from "@/lib/api";
import Link from "next/link";

/* ── Currency metadata ─────────────────────────────────────────────── */

const CURRENCY_META: Record<string, { icon: string; color: string; bg: string; name: string }> = {
  BTC:  { icon: "₿", color: "#f7931a", bg: "#f7931a22", name: "Bitcoin" },
  ETH:  { icon: "Ξ", color: "#8aa0f0", bg: "#627eea22", name: "Ethereum" },
  USDT: { icon: "₮", color: "#3fbf95", bg: "#26a17b22", name: "Tether" },
  SOL:  { icon: "◎", color: "#3fe0a8", bg: "#14f19522", name: "Solana" },
  BNB:  { icon: "B", color: "#f0b90b", bg: "#f0b90b22", name: "BNB" },
  XRP:  { icon: "X", color: "#23a4d0", bg: "#23a4d022", name: "Ripple" },
  ADA:  { icon: "₳", color: "#3cc8c8", bg: "#3cc8c822", name: "Cardano" },
  DOGE: { icon: "Ð", color: "#c2a633", bg: "#c2a63322", name: "Dogecoin" },
  AVAX: { icon: "A", color: "#e84142", bg: "#e8414222", name: "Avalanche" },
  LINK: { icon: "⬡", color: "#2a5ada", bg: "#2a5ada22", name: "Chainlink" },
};

function getMeta(currency: string) {
  return CURRENCY_META[currency] ?? { icon: currency[0], color: "#aeb6c0", bg: "#ffffff10", name: currency };
}

/* ── Mock wallet data used when API returns nothing ────────────────── */

interface WalletRow {
  currency: string;
  total: number;
  available: number;
  inOrder: number;
  valueUsd: number;
  allocation: number;
}

const MOCK_WALLETS: WalletRow[] = [
  { currency: "BTC",  total: 0.9226, available: 0.8726, inOrder: 0.0500, valueUsd: 26913.38, allocation: 45 },
  { currency: "ETH",  total: 5.2800, available: 4.7800, inOrder: 0.5000, valueUsd: 16746.15, allocation: 28 },
  { currency: "USDT", total: 8971.05, available: 8471.05, inOrder: 500.00, valueUsd: 8971.05, allocation: 15 },
  { currency: "SOL",  total: 48.30,  available: 43.30,  inOrder: 5.00,   valueUsd: 7177.10,  allocation: 12 },
];

const TOTAL_VALUE = 59807.68;
const TOTAL_CHANGE = 1284.40;
const TOTAL_CHANGE_PCT = 2.19;
const BTC_EQUIVALENT = 0.9226;

/* ── Allocation donut colors ──────────────────────────────────────── */

const ALLOC_COLORS: Record<string, string> = {
  BTC: "#f7931a",
  ETH: "#627eea",
  USDT: "#26a17b",
  SOL: "#14f195",
};

/* ── Formatting helpers ───────────────────────────────────────────── */

function fmtUsd(n: number): string {
  return n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

function fmtCrypto(value: number, currency: string): string {
  if (currency === "USDT" || currency === "USD") {
    return fmtUsd(value);
  }
  if (value >= 1) return value.toFixed(4);
  return value.toFixed(8).replace(/0+$/, "").replace(/\.$/, ".0");
}

/* ── Component ────────────────────────────────────────────────────── */

export default function WalletPage() {
  const { user, loading: authLoading } = useAuth();
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [hideZero, setHideZero] = useState(false);

  useEffect(() => {
    if (!user) return;
    wallet.list().then(setWallets).catch(() => {});
    const id = setInterval(() => {
      wallet.list().then(setWallets).catch(() => {});
    }, 5000);
    return () => clearInterval(id);
  }, [user]);

  if (authLoading) {
    return (
      <div className="flex flex-col h-screen bg-[#0b0e11]">
        <Header />
        <div className="flex-1 flex items-center justify-center">
          <div className="w-8 h-8 border-2 border-[#2ebd85] border-t-transparent rounded-full animate-spin" />
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div className="flex flex-col h-screen bg-[#0b0e11]">
        <Header />
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <p className="text-[#848e9c] text-base mb-4">Sign in to view your wallet</p>
            <a href="/login" className="inline-block px-6 py-2.5 bg-[#2ebd85] text-[#04130c] text-sm font-semibold rounded-lg hover:bg-[#26a87a] transition-colors">
              Sign In
            </a>
          </div>
        </div>
      </div>
    );
  }

  /* Convert API wallets to WalletRows, or fall back to mock data */
  const rows: WalletRow[] = wallets.length > 0
    ? wallets.map((w) => {
        const total = parseFloat(w.balance) || 0;
        const available = parseFloat(w.available) || 0;
        const inOrder = parseFloat(w.locked_balance) || 0;
        return {
          currency: w.currency,
          total,
          available,
          inOrder,
          valueUsd: 0,
          allocation: 0,
        };
      })
    : MOCK_WALLETS;

  const displayRows = hideZero ? rows.filter((r) => r.total > 0) : rows;

  /* Build conic-gradient for the donut chart */
  const allocEntries = (wallets.length > 0 ? rows : MOCK_WALLETS).filter((r) => r.allocation > 0);
  let conicStops = "";
  let accDeg = 0;
  allocEntries.forEach((r) => {
    const color = ALLOC_COLORS[r.currency] || getMeta(r.currency).color;
    const deg = (r.allocation / 100) * 360;
    conicStops += `${color} ${accDeg}deg ${accDeg + deg}deg, `;
    accDeg += deg;
  });
  conicStops = conicStops.replace(/, $/, "");

  return (
    <div className="flex flex-col h-screen bg-[#0b0e11] overflow-hidden">
      <Header />
      <div className="flex-1 overflow-y-auto min-h-0 p-7">
        {/* Page title */}
        <h1 className="text-[26px] font-bold tracking-[-0.02em] mb-6">Wallet</h1>

        {/* Top section: 2-column grid */}
        <div className="grid gap-4 mb-6" style={{ gridTemplateColumns: "1.4fr 1fr" }}>
          {/* Left card — Total value */}
          <div
            className="rounded-xl p-6"
            style={{
              background: "linear-gradient(135deg, #11201a, #0d1014)",
              border: "1px solid #1f352b",
            }}
          >
            <div className="text-[13px] text-[#7a8794] mb-2">Estimated Total Value</div>
            <div className="flex items-baseline gap-3 mb-1">
              <span className="text-[38px] font-bold font-mono tracking-tight text-foreground">
                ${fmtUsd(TOTAL_VALUE)}
              </span>
              <span className="text-[14px] font-mono text-[#2ebd85]">
                +${fmtUsd(TOTAL_CHANGE)} ({TOTAL_CHANGE_PCT}%)
              </span>
            </div>
            <div className="text-[13px] font-mono text-[#5e6673] mb-5">
              ≈ {BTC_EQUIVALENT.toFixed(4)} BTC
            </div>
            <div className="flex gap-3">
              <button className="py-2.5 px-5 text-[13px] font-semibold bg-[#2ebd85] text-[#04130c] rounded-[9px] hover:bg-[#26a87a] transition-colors">
                Deposit
              </button>
              <button className="py-2.5 px-5 text-[13px] font-semibold text-[#eaecef] bg-transparent border border-[#2a313a] rounded-[9px] hover:bg-[#161a1e] transition-colors">
                Withdraw
              </button>
              <button className="py-2.5 px-5 text-[13px] font-semibold text-[#eaecef] bg-transparent border border-[#2a313a] rounded-[9px] hover:bg-[#161a1e] transition-colors">
                Transfer
              </button>
            </div>
          </div>

          {/* Right card — Allocation donut */}
          <div className="rounded-xl p-6 bg-[#0d1014] border border-[#1c2127]">
            <div className="flex items-center gap-6">
              {/* Donut */}
              <div className="relative shrink-0" style={{ width: 120, height: 120 }}>
                <div
                  className="w-full h-full rounded-full"
                  style={{ background: `conic-gradient(${conicStops})` }}
                />
                <div
                  className="absolute rounded-full bg-[#0d1014] flex items-center justify-center"
                  style={{ width: 78, height: 78, top: 21, left: 21 }}
                >
                  <span className="text-[15px] font-semibold text-[#aeb6c0]">
                    {allocEntries.length} <span className="text-[11px] font-normal text-[#5e6673]">assets</span>
                  </span>
                </div>
              </div>
              {/* Legend */}
              <div className="flex flex-col gap-2.5">
                {allocEntries.map((r) => {
                  const color = ALLOC_COLORS[r.currency] || getMeta(r.currency).color;
                  return (
                    <div key={r.currency} className="flex items-center gap-2">
                      <span className="w-[10px] h-[10px] rounded-full shrink-0" style={{ background: color }} />
                      <span className="text-[13px] text-[#aeb6c0]">{r.currency}</span>
                      <span className="text-[13px] font-mono text-[#5e6673]">{r.allocation}%</span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        </div>

        {/* Balances header row (outside the card) */}
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-base font-semibold">Balances</h2>
          <label className="flex items-center gap-2 cursor-pointer select-none">
            <span className="text-xs text-[#5e6673]">Hide zero balances</span>
            <button
              onClick={() => setHideZero((v) => !v)}
              className="w-8 h-[18px] rounded-full relative transition-colors"
              style={{ background: hideZero ? "#2ebd85" : "#242a31" }}
            >
              <span
                className="absolute top-[2px] w-[14px] h-[14px] rounded-full bg-white transition-all"
                style={{ left: hideZero ? 16 : 2 }}
              />
            </button>
          </label>
        </div>

        {/* Balances table */}
        <div className="bg-[#0d1014] border border-[#1c2127] rounded-xl overflow-hidden">
          {/* Column headers */}
          <div className="flex items-center py-3 px-5 border-b border-[#1c2127] text-[11px] text-[#5e6673] uppercase tracking-wider">
            <span className="flex-[2]">Asset</span>
            <span className="flex-[1.2] text-right">Total</span>
            <span className="flex-[1.2] text-right">Available</span>
            <span className="flex-[1.2] text-right">In Order</span>
            <span className="flex-[1.2] text-right">Value (USD)</span>
            <span className="flex-[1.5] text-right">Allocation</span>
            <span className="w-[140px] text-right">Action</span>
          </div>

          {displayRows.length === 0 && (
            <div className="py-8 text-center text-sm text-[#5e6673]">No wallets yet — deposit to get started</div>
          )}

          {displayRows.map((r) => {
            const meta = getMeta(r.currency);
            return (
              <div key={r.currency} className="flex items-center py-3.5 px-5 border-b border-[#14181d] hover:bg-white/[0.02] transition-colors last:border-0">
                {/* Asset */}
                <span className="flex-[2] flex items-center gap-3">
                  <div
                    className="w-[32px] h-[32px] rounded-full flex items-center justify-center text-sm font-bold shrink-0"
                    style={{ background: meta.bg, color: meta.color }}
                  >
                    {meta.icon}
                  </div>
                  <div className="flex flex-col">
                    <span className="text-sm font-semibold text-foreground">{r.currency}</span>
                    <span className="text-[11px] text-[#5e6673]">{meta.name}</span>
                  </div>
                </span>
                {/* Total */}
                <span className="flex-[1.2] text-right text-sm font-mono text-[#eaecef]">
                  {fmtCrypto(r.total, r.currency)}
                </span>
                {/* Available */}
                <span className="flex-[1.2] text-right text-sm font-mono text-[#eaecef]">
                  {fmtCrypto(r.available, r.currency)}
                </span>
                {/* In Order */}
                <span className="flex-[1.2] text-right text-sm font-mono text-[#5e6673]">
                  {fmtCrypto(r.inOrder, r.currency)}
                </span>
                {/* Value USD */}
                <span className="flex-[1.2] text-right text-sm font-mono text-[#eaecef]">
                  ${fmtUsd(r.valueUsd)}
                </span>
                {/* Allocation bar + % */}
                <span className="flex-[1.5] flex items-center justify-end gap-2">
                  <div className="w-[80px] h-[6px] bg-[#1c2127] rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full"
                      style={{
                        width: `${r.allocation}%`,
                        background: ALLOC_COLORS[r.currency] || meta.color,
                      }}
                    />
                  </div>
                  <span className="text-[12px] font-mono text-[#5e6673] w-[36px] text-right">{r.allocation}%</span>
                </span>
                {/* Actions */}
                <span className="w-[140px] flex justify-end gap-2">
                  <span className="text-[11.5px] font-semibold bg-[#2ebd85]/15 border border-[#2ebd85]/30 rounded-md px-3 py-1.5 text-[#2ebd85] cursor-pointer hover:bg-[#2ebd85]/25 transition-colors">
                    Deposit
                  </span>
                  <Link
                    href={`/trade?pair=${encodeURIComponent(r.currency + "/USDT")}`}
                    className="text-[11.5px] font-semibold bg-[#161a1e] border border-[#2a313a] rounded-md px-3 py-1.5 text-[#eaecef] cursor-pointer hover:bg-[#1c2127] transition-colors"
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
