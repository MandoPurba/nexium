//! Core domain types shared across the order book and engine layers.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: Uuid,
    pub user_id: Uuid,
    pub pair: String,
    pub side: Side,
    pub order_type: OrderType,
    /// `None` for market orders. Limit orders must carry a price.
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub filled_qty: Decimal,
    pub created_at: DateTime<Utc>,
}

impl Order {
    /// Remaining unfilled quantity.
    pub fn remaining(&self) -> Decimal {
        self.quantity - self.filled_qty
    }

    /// True when the order has been fully filled.
    pub fn is_filled(&self) -> bool {
        self.filled_qty >= self.quantity
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub pair: String,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub maker_user_id: Uuid,
    pub taker_user_id: Uuid,
    /// Maker (resting) side.
    pub maker_side: Side,
    /// Price the maker order was sitting at — by convention the trade price.
    pub price: Decimal,
    pub quantity: Decimal,
    pub executed_at: DateTime<Utc>,
}

/// A point-in-time snapshot of one side of the order book — a vec of
/// `(price, total_quantity)` pairs, sorted best-first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    pub pair: String,
    pub bids: Vec<(Decimal, Decimal)>,
    pub asks: Vec<(Decimal, Decimal)>,
}
