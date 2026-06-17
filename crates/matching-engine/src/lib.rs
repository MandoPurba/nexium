//! In-memory order book and matching logic — single-threaded, price-time priority.
//!
//! Two layers:
//!
//! * [`orderbook::OrderBook`] is the pure data structure — a `BTreeMap` of price
//!   levels, each holding a FIFO queue of resting orders. [`OrderBook::process`]
//!   takes an incoming order and returns the trades that resulted from matching
//!   it against the opposite side.
//! * [`engine::Engine`] wires multiple order books (one per trading pair)
//!   together behind an `mpsc` channel, so producers (HTTP handlers) can submit
//!   [`engine::EngineCommand`]s and consumers (settlement task) receive
//!   [`engine::EngineEvent`]s.

pub mod engine;
pub mod orderbook;
pub mod types;

pub use engine::{Engine, EngineCommand, EngineEvent};
pub use orderbook::OrderBook;
pub use types::{Order, OrderType, Side, Trade};
