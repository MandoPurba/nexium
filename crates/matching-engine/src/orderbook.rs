//! Price-time priority limit-order book.
//!
//! Bids and asks are kept in `BTreeMap<Decimal, VecDeque<Order>>`, which sorts
//! price levels in ascending order. Bids match best-first by iterating in
//! reverse; asks match best-first by iterating forward.

use std::collections::{BTreeMap, VecDeque};

use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::types::{Order, OrderType, Side, Trade};

#[derive(Debug)]
pub struct OrderBook {
    pub pair: String,
    /// Bids — keyed by price, sorted ASC; iterate `.iter_mut().rev()` for best-first.
    pub bids: BTreeMap<Decimal, VecDeque<Order>>,
    /// Asks — keyed by price, sorted ASC; iterate `.iter_mut()` for best-first.
    pub asks: BTreeMap<Decimal, VecDeque<Order>>,
}

impl OrderBook {
    pub fn new(pair: impl Into<String>) -> Self {
        Self {
            pair: pair.into(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    /// Best bid (highest buy price), or `None` if no bids.
    pub fn best_bid(&self) -> Option<Decimal> {
        self.bids.keys().next_back().copied()
    }

    /// Best ask (lowest sell price), or `None` if no asks.
    pub fn best_ask(&self) -> Option<Decimal> {
        self.asks.keys().next().copied()
    }

    /// Match `incoming` against the opposite side of the book and return the
    /// trades that resulted. Any unfilled remainder of a limit order is
    /// inserted into the book at its limit price. Unfilled market orders are
    /// discarded (no resting price), per the matching engine spec.
    pub fn process(&mut self, mut incoming: Order) -> Vec<Trade> {
        let mut trades = Vec::new();

        match incoming.side {
            Side::Buy => self.match_buy(&mut incoming, &mut trades),
            Side::Sell => self.match_sell(&mut incoming, &mut trades),
        }

        // Insert any unfilled remainder of a limit order back into the book.
        if !incoming.is_filled() && incoming.order_type == OrderType::Limit {
            if let Some(price) = incoming.price {
                let book = match incoming.side {
                    Side::Buy => &mut self.bids,
                    Side::Sell => &mut self.asks,
                };
                book.entry(price).or_default().push_back(incoming);
            }
        }

        trades
    }

    /// Cancel a resting order by id. Returns the removed order, or `None` if
    /// it wasn't in the book.
    pub fn cancel(&mut self, order_id: Uuid) -> Option<Order> {
        for book in [&mut self.bids, &mut self.asks] {
            let mut empty_price = None;
            for (price, queue) in book.iter_mut() {
                if let Some(pos) = queue.iter().position(|o| o.id == order_id) {
                    let removed = queue.remove(pos);
                    if queue.is_empty() {
                        empty_price = Some(*price);
                    }
                    if let Some(p) = empty_price {
                        book.remove(&p);
                    }
                    return removed;
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------
    // Side-specific matching
    // -----------------------------------------------------------------

    fn match_buy(&mut self, taker: &mut Order, trades: &mut Vec<Trade>) {
        let mut empty_levels: Vec<Decimal> = Vec::new();

        // Asks are sorted ascending — iterate forward for best (lowest) first.
        for (ask_price, queue) in self.asks.iter_mut() {
            // Limit buys only cross at or above their price.
            if let Some(taker_price) = taker.price {
                if taker_price < *ask_price {
                    break;
                }
            }
            if taker.is_filled() {
                break;
            }

            fill_against_queue(taker, queue, *ask_price, trades);

            if queue.is_empty() {
                empty_levels.push(*ask_price);
            }
            if taker.is_filled() {
                break;
            }
        }

        for p in empty_levels {
            self.asks.remove(&p);
        }
    }

    fn match_sell(&mut self, taker: &mut Order, trades: &mut Vec<Trade>) {
        let mut empty_levels: Vec<Decimal> = Vec::new();

        // Bids are sorted ascending — iterate in reverse for best (highest) first.
        for (bid_price, queue) in self.bids.iter_mut().rev() {
            if let Some(taker_price) = taker.price {
                if taker_price > *bid_price {
                    break;
                }
            }
            if taker.is_filled() {
                break;
            }

            fill_against_queue(taker, queue, *bid_price, trades);

            if queue.is_empty() {
                empty_levels.push(*bid_price);
            }
            if taker.is_filled() {
                break;
            }
        }

        for p in empty_levels {
            self.bids.remove(&p);
        }
    }
}

/// Drain `queue` (FIFO) into the taker, emitting trades for each fill.
fn fill_against_queue(
    taker: &mut Order,
    queue: &mut VecDeque<Order>,
    maker_price: Decimal,
    trades: &mut Vec<Trade>,
) {
    while !taker.is_filled() {
        let Some(maker) = queue.front_mut() else {
            break;
        };

        // Self-trade prevention: skip the maker (pop it) without trading.
        // This is a simple policy; production engines often choose
        // cancel-newest or decrement-and-cancel instead.
        if maker.user_id == taker.user_id {
            queue.pop_front();
            continue;
        }

        let fill_qty = taker.remaining().min(maker.remaining());
        if fill_qty <= Decimal::ZERO {
            // Shouldn't happen with positive quantity invariants — bail safely.
            break;
        }

        trades.push(Trade {
            id: Uuid::new_v4(),
            pair: taker.pair.clone(),
            maker_order_id: maker.id,
            taker_order_id: taker.id,
            maker_user_id: maker.user_id,
            taker_user_id: taker.user_id,
            maker_side: maker.side,
            price: maker_price,
            quantity: fill_qty,
            executed_at: Utc::now(),
        });

        taker.filled_qty += fill_qty;
        maker.filled_qty += fill_qty;

        if maker.is_filled() {
            queue.pop_front();
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    fn dec(s: &str) -> Decimal {
        Decimal::from_str_exact(s).unwrap()
    }

    #[test]
    fn no_match_when_book_is_empty() {
        let mut ob = OrderBook::new("BTC/USDT");
        let trades = ob.process(order(
            Side::Buy,
            Some(dec("65000")),
            dec("0.5"),
            Uuid::new_v4(),
        ));
        assert!(trades.is_empty());
        // Order rests in the book.
        assert_eq!(ob.bids.len(), 1);
        assert_eq!(ob.best_bid(), Some(dec("65000")));
    }

    #[test]
    fn no_match_when_prices_dont_cross() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Alice asks 65100.
        let trades = ob.process(order(Side::Sell, Some(dec("65100")), dec("1"), alice));
        assert!(trades.is_empty());

        // Bob bids 65000 — below the best ask, so no match.
        let trades = ob.process(order(Side::Buy, Some(dec("65000")), dec("1"), bob));
        assert!(trades.is_empty());

        assert_eq!(ob.best_bid(), Some(dec("65000")));
        assert_eq!(ob.best_ask(), Some(dec("65100")));
    }

    #[test]
    fn full_match_clears_both_orders() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Maker: Alice ask 1 BTC at 65000.
        ob.process(order(Side::Sell, Some(dec("65000")), dec("1"), alice));

        // Taker: Bob buys 1 BTC at 65100 (crosses) → match at maker price 65000.
        let trades = ob.process(order(Side::Buy, Some(dec("65100")), dec("1"), bob));

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].price, dec("65000"));
        assert_eq!(trades[0].quantity, dec("1"));
        assert_eq!(trades[0].maker_user_id, alice);
        assert_eq!(trades[0].taker_user_id, bob);

        // Book is empty after a full match.
        assert!(ob.asks.is_empty());
        assert!(ob.bids.is_empty());
    }

    #[test]
    fn taker_partial_fill_leaves_remainder_in_book() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Maker: Alice ask 0.4 BTC at 65000.
        ob.process(order(Side::Sell, Some(dec("65000")), dec("0.4"), alice));

        // Taker: Bob buys 1 BTC at 65100 → fills 0.4, 0.6 rests as a bid at 65100.
        let trades = ob.process(order(Side::Buy, Some(dec("65100")), dec("1"), bob));

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, dec("0.4"));

        // 0.4 ask consumed; 0.6 remainder rests as bid.
        assert!(ob.asks.is_empty());
        assert_eq!(ob.best_bid(), Some(dec("65100")));
        let rest = &ob.bids[&dec("65100")][0];
        assert_eq!(rest.remaining(), dec("0.6"));
    }

    #[test]
    fn maker_partial_fill_stays_at_front_of_queue() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Maker: Alice ask 1 BTC at 65000.
        ob.process(order(Side::Sell, Some(dec("65000")), dec("1"), alice));

        // Taker: Bob buys 0.3 BTC at 65000 → maker keeps 0.7 in book.
        let trades = ob.process(order(Side::Buy, Some(dec("65000")), dec("0.3"), bob));

        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].quantity, dec("0.3"));

        assert_eq!(ob.bids.len(), 0);
        assert_eq!(ob.best_ask(), Some(dec("65000")));
        let rest = &ob.asks[&dec("65000")][0];
        assert_eq!(rest.remaining(), dec("0.7"));
    }

    #[test]
    fn multi_fill_sweeps_multiple_price_levels() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let carol = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Two ask levels: 0.5 @ 65000 (Alice), 0.5 @ 65100 (Carol).
        ob.process(order(Side::Sell, Some(dec("65000")), dec("0.5"), alice));
        ob.process(order(Side::Sell, Some(dec("65100")), dec("0.5"), carol));

        // Bob buys 1 BTC at 65200 → sweeps both levels.
        let trades = ob.process(order(Side::Buy, Some(dec("65200")), dec("1"), bob));

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].price, dec("65000"));
        assert_eq!(trades[0].quantity, dec("0.5"));
        assert_eq!(trades[1].price, dec("65100"));
        assert_eq!(trades[1].quantity, dec("0.5"));

        assert!(ob.asks.is_empty());
        assert!(ob.bids.is_empty());
    }

    #[test]
    fn fifo_time_priority_at_same_price() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let carol = Uuid::new_v4();
        let bob = Uuid::new_v4();

        // Two asks at the same price; Alice first, Carol second.
        ob.process(order(Side::Sell, Some(dec("65000")), dec("0.4"), alice));
        ob.process(order(Side::Sell, Some(dec("65000")), dec("0.4"), carol));

        // Bob takes 0.5 — must consume Alice's entire 0.4 before touching Carol.
        let trades = ob.process(order(Side::Buy, Some(dec("65000")), dec("0.5"), bob));

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].maker_user_id, alice);
        assert_eq!(trades[0].quantity, dec("0.4"));
        assert_eq!(trades[1].maker_user_id, carol);
        assert_eq!(trades[1].quantity, dec("0.1"));

        // Carol's remainder (0.3) stays at the head of the queue.
        let queue = &ob.asks[&dec("65000")];
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].user_id, carol);
        assert_eq!(queue[0].remaining(), dec("0.3"));
    }

    #[test]
    fn self_trade_prevention_skips_own_orders() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();

        // Alice's own ask.
        ob.process(order(Side::Sell, Some(dec("65000")), dec("1"), alice));

        // Alice tries to buy from herself.
        let trades = ob.process(order(Side::Buy, Some(dec("65000")), dec("1"), alice));
        assert!(trades.is_empty(), "should not trade against self");

        // Her resting ask was popped (per the skip policy) and a fresh bid
        // gets rested for the taker.
        assert!(ob.asks.is_empty());
        assert_eq!(ob.best_bid(), Some(dec("65000")));
    }

    #[test]
    fn market_buy_consumes_best_asks_until_filled() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();
        let bob = Uuid::new_v4();

        ob.process(order(Side::Sell, Some(dec("65000")), dec("0.3"), alice));
        ob.process(order(Side::Sell, Some(dec("65100")), dec("0.5"), alice));

        // Market buy 0.4 → fills 0.3 at 65000 and 0.1 at 65100.
        let trades = ob.process(order(Side::Buy, None, dec("0.4"), bob));
        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].price, dec("65000"));
        assert_eq!(trades[0].quantity, dec("0.3"));
        assert_eq!(trades[1].price, dec("65100"));
        assert_eq!(trades[1].quantity, dec("0.1"));

        // Market remainder is discarded (no resting price).
        assert!(ob.bids.is_empty());
        // 0.4 ask remainder still rests.
        assert_eq!(ob.best_ask(), Some(dec("65100")));
        assert_eq!(ob.asks[&dec("65100")][0].remaining(), dec("0.4"));
    }

    #[test]
    fn cancel_removes_resting_order_and_collapses_empty_level() {
        let mut ob = OrderBook::new("BTC/USDT");
        let alice = Uuid::new_v4();

        let o = order(Side::Sell, Some(dec("65000")), dec("1"), alice);
        let id = o.id;
        ob.process(o);
        assert_eq!(ob.asks.len(), 1);

        let removed = ob.cancel(id).expect("must cancel");
        assert_eq!(removed.id, id);
        assert!(ob.asks.is_empty(), "empty price level should be collapsed");

        // Cancelling again returns None.
        assert!(ob.cancel(id).is_none());
    }
}
