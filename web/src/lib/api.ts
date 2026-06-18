const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8084";
const WS_URL = process.env.NEXT_PUBLIC_WS_URL || "ws://localhost:8084";

export { WS_URL };

export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

async function request<T>(
  url: string,
  options?: RequestInit
): Promise<T> {
  const token =
    typeof window !== "undefined" ? localStorage.getItem("token") : null;

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    ...(options?.headers as Record<string, string>),
  };

  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(url, { ...options, headers });

  if (!res.ok) {
    const error: ApiError = await res.json().catch(() => ({
      code: "NETWORK_ERROR",
      message: res.statusText,
    }));
    throw error;
  }

  if (res.status === 204) return undefined as T;
  return res.json();
}

// Auth
export interface UserResponse {
  id: string;
  email: string;
  status: string;
  created_at: string;
}

export interface LoginResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
}

export interface MeResponse {
  id: string;
  email: string;
  status: string;
  kyc_level: string;
}

export const auth = {
  register: (email: string, password: string) =>
    request<UserResponse>(`${API_URL}/auth/register`, {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),

  login: (email: string, password: string) =>
    request<LoginResponse>(`${API_URL}/auth/login`, {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),

  me: () => request<MeResponse>(`${API_URL}/auth/me`),
};

// Wallet
export interface Wallet {
  id: string;
  currency: string;
  balance: string;
  locked_balance: string;
  available: string;
}

export const wallet = {
  list: () => request<Wallet[]>(`${API_URL}/wallets`),

  get: (currency: string) =>
    request<Wallet>(`${API_URL}/wallets/${currency}`),

  deposit: (currency: string, amount: string) =>
    request<{ txn_id: string; currency: string; amount: string; status: string }>(
      `${API_URL}/wallets/deposit`,
      { method: "POST", body: JSON.stringify({ currency, amount }) }
    ),
};

// Trading
export interface TradingPair {
  symbol: string;
  base_currency: string;
  quote_currency: string;
  min_qty: string;
  tick_size: string;
}

export interface Order {
  id: string;
  pair: string;
  side: string;
  type: string;
  status: string;
  price: string | null;
  quantity: string;
  filled_qty: string;
  created_at: string;
}

export interface PlaceOrderParams {
  pair: string;
  side: "buy" | "sell";
  type: "limit" | "market";
  price?: string;
  quantity: string;
}

export const trading = {
  pairs: () => request<TradingPair[]>(`${API_URL}/pairs`),

  placeOrder: (params: PlaceOrderParams) =>
    request<Order>(`${API_URL}/orders`, {
      method: "POST",
      body: JSON.stringify(params),
    }),

  listOrders: (params?: { pair?: string; status?: string; limit?: number }) => {
    const search = new URLSearchParams();
    if (params?.pair) search.set("pair", params.pair);
    if (params?.status) search.set("status", params.status);
    if (params?.limit) search.set("limit", String(params.limit));
    const qs = search.toString();
    return request<Order[]>(`${API_URL}/orders${qs ? `?${qs}` : ""}`);
  },

  getOrder: (id: string) => request<Order>(`${API_URL}/orders/${id}`),

  cancelOrder: (id: string) =>
    request<{ id: string; status: string }>(`${API_URL}/orders/${id}`, {
      method: "DELETE",
    }),
};

// Market Data
export interface OhlcvCandle {
  pair: string;
  interval: string;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string;
  bucket: string;
}

export interface TradeRecord {
  id: string;
  pair: string;
  price: string;
  quantity: string;
  side: string;
  executed_at: string;
}

export interface OrderBookSnapshot {
  pair: string;
  bids: [string, string][];
  asks: [string, string][];
  timestamp: string;
}

export const market = {
  ohlcv: (pair: string, interval = "1h", limit = 100) =>
    request<OhlcvCandle[]>(
      `${API_URL}/market/ohlcv?pair=${encodeURIComponent(pair)}&interval=${interval}&limit=${limit}`
    ),

  orderbook: (pair: string) =>
    request<OrderBookSnapshot>(
      `${API_URL}/market/orderbook/${pair.replace("/", "-")}`
    ),

  trades: (pair: string, limit = 50) =>
    request<TradeRecord[]>(
      `${API_URL}/market/trades/${pair.replace("/", "-")}?limit=${limit}`
    ),
};
