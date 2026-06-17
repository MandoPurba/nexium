//! Multi-pair engine runner.
//!
//! Owns one [`OrderBook`] per trading pair and serializes all state mutations
//! through a single `tokio::sync::mpsc` consumer. Producers (HTTP handlers)
//! send [`EngineCommand`]; consumers (settlement task) receive [`EngineEvent`].

use std::collections::HashMap;

use tokio::sync::mpsc;
use uuid::Uuid;

use crate::orderbook::OrderBook;
use crate::types::{Order, Trade};

#[derive(Debug, Clone)]
pub enum EngineCommand {
    /// Match this order against the book; rest the remainder if it's a limit order.
    PlaceOrder(Order),
    /// Cancel a resting order by id (pair is required so we don't scan every book).
    CancelOrder { order_id: Uuid, pair: String },
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// Emitted after a `PlaceOrder` is processed — includes the taker's final
    /// fill state along with each trade that resulted (possibly empty).
    OrderProcessed { taker: Order, trades: Vec<Trade> },
    /// Emitted after a `CancelOrder` — `Some(order)` when removed, `None` if
    /// the order wasn't in the book.
    OrderCancelled {
        order_id: Uuid,
        removed: Option<Order>,
    },
}

/// Multi-pair engine. Held inside the runner task; not `Clone`.
pub struct Engine {
    books: HashMap<String, OrderBook>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
        }
    }

    fn book_for(&mut self, pair: &str) -> &mut OrderBook {
        self.books
            .entry(pair.to_string())
            .or_insert_with(|| OrderBook::new(pair))
    }

    /// Run loop — pulls commands until the producer side is dropped. Events
    /// are sent on `event_tx`; if the receiver is gone we keep matching but
    /// stop publishing.
    pub async fn run(
        mut self,
        mut cmd_rx: mpsc::Receiver<EngineCommand>,
        event_tx: mpsc::Sender<EngineEvent>,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                EngineCommand::PlaceOrder(order) => {
                    let pair = order.pair.clone();
                    let book = self.book_for(&pair);
                    // Drive the match. Trades are collected; the remainder
                    // (if any) is automatically rested by `process`.
                    //
                    // We need to report the taker's *post-match* state to the
                    // settlement layer so it can compute filled_qty deltas.
                    // The pure book consumes the order by value, so we clone
                    // metadata first and reconstruct the taker view from
                    // trades.
                    let taker_id = order.id;
                    let mut taker_view = order.clone();
                    let trades = book.process(order);
                    for t in &trades {
                        if t.taker_order_id == taker_id {
                            taker_view.filled_qty += t.quantity;
                        }
                    }
                    let _ = event_tx
                        .send(EngineEvent::OrderProcessed {
                            taker: taker_view,
                            trades,
                        })
                        .await;
                }
                EngineCommand::CancelOrder { order_id, pair } => {
                    let removed = self.book_for(&pair).cancel(order_id);
                    let _ = event_tx
                        .send(EngineEvent::OrderCancelled { order_id, removed })
                        .await;
                }
            }
        }

        tracing::info!("matching engine command channel closed; shutting down");
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{OrderType, Side};
    use chrono::Utc;
    use rust_decimal::Decimal;

    fn order(side: Side, price: Option<Decimal>, qty: Decimal, user: Uuid) -> Order {
        Order {
            id: Uuid::new_v4(),
            user_id: user,
            pair: "BTC/USDT".into(),
            side,
            order_type: if price.is_some() {
                OrderType::Limit
            } else {
                OrderType::Market
            },
            price,
            quantity: qty,
            filled_qty: Decimal::ZERO,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn engine_matches_two_orders_via_channel() {
        let (cmd_tx, cmd_rx) = mpsc::channel(8);
        let (evt_tx, mut evt_rx) = mpsc::channel(8);

        let engine = Engine::new();
        let handle = tokio::spawn(engine.run(cmd_rx, evt_tx));

        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        cmd_tx
            .send(EngineCommand::PlaceOrder(order(
                Side::Sell,
                Some(Decimal::from(65000)),
                Decimal::from(1),
                alice,
            )))
            .await
            .unwrap();
        cmd_tx
            .send(EngineCommand::PlaceOrder(order(
                Side::Buy,
                Some(Decimal::from(65000)),
                Decimal::from(1),
                bob,
            )))
            .await
            .unwrap();

        // First event: Alice's ask rests (no trade).
        let evt = evt_rx.recv().await.unwrap();
        match evt {
            EngineEvent::OrderProcessed { trades, .. } => assert!(trades.is_empty()),
            _ => panic!("expected OrderProcessed"),
        }

        // Second event: Bob's buy fully matches.
        let evt = evt_rx.recv().await.unwrap();
        match evt {
            EngineEvent::OrderProcessed { taker, trades } => {
                assert_eq!(trades.len(), 1);
                assert_eq!(trades[0].quantity, Decimal::from(1));
                assert_eq!(taker.filled_qty, Decimal::from(1));
            }
            _ => panic!("expected OrderProcessed"),
        }

        drop(cmd_tx);
        handle.await.unwrap();
    }
}
