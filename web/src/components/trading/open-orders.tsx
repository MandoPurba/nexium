"use client";

import { useEffect, useState, useCallback } from "react";
import { trading, Order, ApiError } from "@/lib/api";
import { formatPrice, formatAmount } from "@/lib/utils";
import { X } from "lucide-react";

function FillBar({ filled, total }: { filled: string; total: string }) {
  const f = parseFloat(filled);
  const t = parseFloat(total);
  const pct = t > 0 ? Math.min((f / t) * 100, 100) : 0;
  return (
    <div className="flex items-center gap-1.5">
      <div className="flex-1 h-1 bg-white/10 rounded-full overflow-hidden">
        <div
          className="h-full bg-[#2ebd85]/60 rounded-full transition-all"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="text-[10px] text-[#5e6673] font-mono w-8 text-right">
        {pct.toFixed(0)}%
      </span>
    </div>
  );
}

export function OpenOrders({
  pair,
  refreshKey,
}: {
  pair: string;
  refreshKey: number;
}) {
  const [orders, setOrders] = useState<Order[]>([]);
  const [cancellingId, setCancellingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await trading.listOrders({ pair, status: "open", limit: 20 });
      setOrders(data);
    } catch { /* ignore */ }
  }, [pair]);

  useEffect(() => { load(); }, [load, refreshKey]);
  useEffect(() => {
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  }, [load]);

  async function handleCancel(id: string) {
    setCancellingId(id);
    try {
      await trading.cancelOrder(id);
      load();
    } catch (err) {
      alert((err as ApiError).message || "Cancel failed");
    } finally {
      setCancellingId(null);
    }
  }

  return (
    <div className="flex flex-col bg-[#0d1014]">
      {/* Header */}
      <div className="flex items-center justify-between px-3.5 py-2.5 border-b border-[#1c2127] shrink-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-semibold text-[#eaecef]">Open Orders</span>
          {orders.length > 0 && (
            <span className="text-[10px] bg-[#2ebd85]/15 text-[#2ebd85] px-1.5 py-0.5 rounded font-mono">
              {orders.length}
            </span>
          )}
        </div>
      </div>

      {orders.length === 0 ? (
        <p className="text-xs text-[#5e6673] px-3.5 py-4 text-center">No open orders</p>
      ) : (
        <div className="overflow-x-auto">
          {/* Table header */}
          <div
            className="grid text-[10.5px] text-[#5e6673] px-3.5 py-1.5 border-b border-[#1c2127]/50 min-w-[420px]"
            style={{ gridTemplateColumns: "1fr 50px 70px 80px 80px 60px 32px" }}
          >
            <span>Pair</span>
            <span>Type</span>
            <span>Side</span>
            <span className="text-right">Price</span>
            <span className="text-right">Amount</span>
            <span className="text-right pr-2">Filled</span>
            <span />
          </div>

          {orders.map((o) => (
            <div
              key={o.id}
              className="grid items-center px-3.5 py-2 border-b border-[#1c2127]/30 hover:bg-white/[0.03] last:border-0 min-w-[420px]"
              style={{ gridTemplateColumns: "1fr 50px 70px 80px 80px 60px 32px" }}
            >
              <span className="text-[12px] font-medium text-[#eaecef]">{o.pair}</span>

              <span className={`text-[10.5px] px-1.5 py-0.5 rounded capitalize w-fit
                ${o.type === "limit" ? "bg-[#242a31] text-[#848e9c]" : "bg-[#2ebd85]/10 text-[#2ebd85]"}`}>
                {o.type}
              </span>

              <span className={`text-[12px] font-semibold ${o.side === "buy" ? "text-[#2ebd85]" : "text-[#f6465d]"}`}>
                {o.side === "buy" ? "▲ Buy" : "▼ Sell"}
              </span>

              <span className="text-right text-[12px] font-mono text-[#eaecef]">
                {o.price ? formatPrice(o.price) : <span className="text-[#5e6673]">Market</span>}
              </span>

              <span className="text-right text-[12px] font-mono text-[#eaecef]">
                {formatAmount(o.quantity)}
              </span>

              <div className="pr-2">
                <FillBar filled={o.filled_qty} total={o.quantity} />
              </div>

              <button
                type="button"
                className="flex items-center justify-center w-6 h-6 rounded text-[#5e6673] hover:text-[#f6465d] hover:bg-[#f6465d]/10 transition-colors disabled:opacity-40"
                disabled={cancellingId === o.id}
                onClick={() => handleCancel(o.id)}
                title="Cancel order"
              >
                {cancellingId === o.id ? (
                  <span className="text-[10px]">…</span>
                ) : (
                  <X className="w-3 h-3" />
                )}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
