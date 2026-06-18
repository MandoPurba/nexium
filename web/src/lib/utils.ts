import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatPrice(value: string, decimals = 2): string {
  const n = parseFloat(value);
  if (isNaN(n)) return value;
  return n.toLocaleString("en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

export function formatAmount(value: string, decimals = 6): string {
  const n = parseFloat(value);
  if (isNaN(n)) return value;
  // trim trailing zeros but keep at least 4 decimal places
  const fixed = n.toFixed(decimals);
  const trimmed = fixed.replace(/\.?0+$/, "");
  const parts = trimmed.split(".");
  if (!parts[1] || parts[1].length < 4) {
    return n.toFixed(4);
  }
  return trimmed;
}
