"use client";

import { useState } from "react";
import { trading, ApiError, PlaceOrderParams } from "@/lib/api";

const PCT_STEPS = [25, 50, 75, 100];

export function OrderForm({
  pair,
  onOrderPlaced,
}: {
  pair: string;
  onOrderPlaced?: () => void;
}) {
  const [side, setSide] = useState<"buy" | "sell">("buy");
  const [orderType, setOrderType] = useState<"limit" | "market">("limit");
  const [price, setPrice] = useState("");
  const [quantity, setQuantity] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const base = pair.split("/")[0];
  const quote = pair.split("/")[1];

  const estimatedTotal =
    orderType === "limit" && price && quantity
      ? parseFloat(price) * parseFloat(quantity)
      : null;

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      const params: PlaceOrderParams = { pair, side, type: orderType, quantity };
      if (orderType === "limit") params.price = price;
      await trading.placeOrder(params);
      setPrice("");
      setQuantity("");
      onOrderPlaced?.();
    } catch (err) {
      setError((err as ApiError).message || "Order failed");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col bg-[#0d1014]">
      {/* Buy / Sell tabs */}
      <div className="flex border-b border-[#1c2127]">
        <button
          type="button"
          onClick={() => setSide("buy")}
          className={`flex-1 py-3 text-sm font-semibold transition-colors border-b-2 ${
            side === "buy"
              ? "text-[#2ebd85] border-[#2ebd85]"
              : "text-[#5e6673] border-transparent hover:text-[#848e9c]"
          }`}
        >
          Buy
        </button>
        <button
          type="button"
          onClick={() => setSide("sell")}
          className={`flex-1 py-3 text-sm font-semibold transition-colors border-b-2 ${
            side === "sell"
              ? "text-[#f6465d] border-[#f6465d]"
              : "text-[#5e6673] border-transparent hover:text-[#848e9c]"
          }`}
        >
          Sell
        </button>
      </div>

      <div className="p-4 space-y-3">
        {/* Limit / Market / Stop tabs */}
        <div className="flex gap-4 border-b border-[#1c2127] pb-2">
          {(["limit", "market"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setOrderType(t)}
              className={`text-sm font-semibold pb-2 capitalize transition-colors border-b-2 -mb-2.5 ${
                orderType === t
                  ? "text-foreground border-[#2ebd85]"
                  : "text-[#5e6673] border-transparent hover:text-[#848e9c]"
              }`}
            >
              {t}
            </button>
          ))}
          <button
            type="button"
            className="text-sm font-semibold pb-2 text-[#5e6673] border-b-2 border-transparent -mb-2.5 opacity-50 cursor-not-allowed"
          >
            Stop
          </button>
        </div>

        {/* Available balance */}
        <div className="flex justify-between text-xs">
          <span className="text-[#5e6673]">Available</span>
          <span className="font-mono text-[#aeb6c0]">
            {side === "buy" ? "—— USDT" : `—— ${base}`}
          </span>
        </div>

        <form onSubmit={handleSubmit} className="space-y-3">
          {error && (
            <div className="rounded-lg bg-[#f6465d]/10 px-3 py-2 text-xs text-[#f6465d]">
              {error}
            </div>
          )}

          {/* Price input */}
          {orderType === "limit" && (
            <div className="space-y-1.5">
              <label className="text-[11px] text-[#5e6673]">Price</label>
              <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded-lg px-3 h-[42px] focus-within:border-[#2ebd85]/60 transition-colors">
                <input
                  type="text"
                  inputMode="decimal"
                  placeholder="0.00"
                  value={price}
                  onChange={(e) => setPrice(e.target.value)}
                  required
                  className="flex-1 bg-transparent border-none text-sm font-mono text-foreground focus:outline-none placeholder:text-[#5e6673]"
                />
                <span className="text-xs text-[#5e6673] shrink-0">{quote}</span>
              </div>
            </div>
          )}

          {/* Amount input */}
          <div className="space-y-1.5">
            <label className="text-[11px] text-[#5e6673]">Amount</label>
            <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded-lg px-3 h-[42px] focus-within:border-[#2ebd85]/60 transition-colors">
              <input
                type="text"
                inputMode="decimal"
                placeholder="0.0000"
                value={quantity}
                onChange={(e) => setQuantity(e.target.value)}
                required
                className="flex-1 bg-transparent border-none text-sm font-mono text-foreground focus:outline-none placeholder:text-[#5e6673]"
              />
              <span className="text-xs text-[#5e6673] shrink-0">{base}</span>
            </div>
          </div>

          {/* Percentage quick-fill */}
          <div className="grid grid-cols-4 gap-1.5">
            {PCT_STEPS.map((pct) => (
              <button
                key={pct}
                type="button"
                className="text-[11px] py-1.5 rounded border border-[#242a31] text-[#848e9c] hover:border-[#2ebd85]/50 hover:text-[#2ebd85] transition-colors"
              >
                {pct}%
              </button>
            ))}
          </div>

          {/* Total input */}
          <div className="space-y-1.5">
            <label className="text-[11px] text-[#5e6673]">Total</label>
            <div className="flex items-center bg-[#11151a] border border-[#242a31] rounded-lg px-3 h-[42px]">
              <input
                type="text"
                readOnly
                placeholder="0.00"
                value={
                  estimatedTotal !== null && !isNaN(estimatedTotal)
                    ? estimatedTotal.toLocaleString("en-US", {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      })
                    : ""
                }
                className="flex-1 bg-transparent border-none text-sm font-mono text-[#aeb6c0] focus:outline-none placeholder:text-[#5e6673] cursor-default"
              />
              <span className="text-xs text-[#5e6673] shrink-0">{quote}</span>
            </div>
          </div>

          {/* Submit button */}
          <button
            type="submit"
            disabled={loading}
            className={`w-full py-3 text-sm font-semibold rounded-lg transition-colors ${
              side === "buy"
                ? "bg-[#2ebd85] text-[#04130c] hover:bg-[#26a87a]"
                : "bg-[#f6465d] text-white hover:bg-[#d93850]"
            } disabled:opacity-60`}
          >
            {loading ? "Placing…" : `${side === "buy" ? "Buy" : "Sell"} ${base}`}
          </button>

          {/* Fee estimate */}
          <div className="flex justify-between text-[11px] text-[#5e6673]">
            <span>Est. Fee</span>
            <span className="font-mono">0.10% · — USDT</span>
          </div>
        </form>
      </div>
    </div>
  );
}
