"use client";

import { useEffect, useState } from "react";
import { wallet, Wallet } from "@/lib/api";

function formatBalance(value: string, currency: string): string {
  const n = parseFloat(value);
  if (isNaN(n)) return value;
  if (currency === "USDT" || currency === "USD") {
    return n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  }
  // Trim trailing zeros, keep min 4 decimal places
  const s = n.toFixed(8).replace(/\.?0+$/, "");
  const parts = s.split(".");
  if (!parts[1] || parts[1].length < 4) return n.toFixed(4);
  return s;
}

const CURRENCY_CONFIG: Record<string, { color: string; bg: string; char: string }> = {
  BTC:  { color: "#f7931a", bg: "#f7931a22", char: "₿" },
  ETH:  { color: "#8aa0f0", bg: "#627eea22", char: "Ξ" },
  USDT: { color: "#3fbf95", bg: "#26a17b22", char: "₮" },
  SOL:  { color: "#3fe0a8", bg: "#14f19522", char: "◎" },
  BNB:  { color: "#f0b90b", bg: "#f0b90b22", char: "B" },
};

export function WalletPanel() {
  const [wallets, setWallets] = useState<Wallet[]>([]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const data = await wallet.list();
        if (!cancelled) setWallets(data);
      } catch { /* ignore */ }
    }
    load();
    const id = setInterval(load, 5000);
    return () => { cancelled = true; clearInterval(id); };
  }, []);

  return (
    <div className="bg-[#0d1014] border-t border-[#1c2127]">
      {/* Header */}
      <div className="px-3.5 py-2.5 border-b border-[#1c2127]">
        <span className="text-sm font-semibold text-[#eaecef]">Balances</span>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-3 text-[10.5px] text-[#5e6673] px-3.5 py-1.5 border-b border-[#1c2127]/50">
        <span>Asset</span>
        <span className="text-right">Available</span>
        <span className="text-right">Locked</span>
      </div>

      {/* Wallet rows */}
      <div>
        {wallets.length === 0 && (
          <p className="text-xs text-[#5e6673] px-3.5 py-3">No wallets</p>
        )}
        {wallets.map((w) => {
          const cfg = CURRENCY_CONFIG[w.currency] ?? { color: "#aeb6c0", bg: "#ffffff10", char: w.currency[0] };
          return (
            <div
              key={w.id}
              className="grid grid-cols-3 items-center py-2 px-3.5 hover:bg-white/[0.03] border-b border-[#1c2127]/30 last:border-0"
            >
              {/* Asset */}
              <div className="flex items-center gap-2">
                <div
                  className="w-6 h-6 rounded-full flex items-center justify-center text-[10px] font-bold shrink-0"
                  style={{ background: cfg.bg, color: cfg.color }}
                >
                  {cfg.char}
                </div>
                <span className="text-[13px] font-semibold" style={{ color: cfg.color }}>
                  {w.currency}
                </span>
              </div>

              {/* Available */}
              <div className="text-right">
                <span className="text-[12px] font-mono text-[#eaecef]">
                  {formatBalance(w.available, w.currency)}
                </span>
              </div>

              {/* Locked */}
              <div className="text-right">
                <span className="text-[12px] font-mono text-[#5e6673]">
                  {formatBalance(w.locked_balance, w.currency)}
                </span>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
