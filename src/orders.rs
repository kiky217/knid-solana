//! Deterministic paper-only order evaluation. No wallet keys or live submission.
use crate::{MarketError, PaperAccount};
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderKind {
    Market,
    Limit { limit: f64 },
    Stop { stop: f64 },
    StopLimit { stop: f64, limit: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperOrder {
    pub side: Side,
    pub kind: OrderKind,
    pub fraction: f64,
    pub fee_rate: f64,
    pub max_slippage_bps: u16,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrderStatus {
    Pending,
    Filled { quantity: f64, price: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrderEngine {
    pub account: PaperAccount,
}
impl OrderEngine {
    pub fn new(cash: f64) -> Result<Self, MarketError> {
        Ok(Self {
            account: PaperAccount::new(cash)?,
        })
    }
    pub fn submit(&mut self, order: PaperOrder, mid: f64) -> Result<OrderStatus, MarketError> {
        if !mid.is_finite() || mid <= 0.0 {
            return Err(MarketError::InvalidPrice);
        }
        if !order.fraction.is_finite() || !(0.0..=1.0).contains(&order.fraction) {
            return Err(MarketError::InvalidFraction);
        }
        if order.max_slippage_bps > 10000 {
            return Err(MarketError::InvalidFraction);
        }
        let trigger = match order.kind {
            OrderKind::Market => true,
            OrderKind::Limit { limit } => {
                if !limit.is_finite() || limit <= 0.0 {
                    return Err(MarketError::InvalidPrice);
                }
                match order.side {
                    Side::Buy => mid <= limit,
                    Side::Sell => mid >= limit,
                }
            }
            OrderKind::Stop { stop } => {
                if !stop.is_finite() || stop <= 0.0 {
                    return Err(MarketError::InvalidPrice);
                }
                match order.side {
                    Side::Buy => mid >= stop,
                    Side::Sell => mid <= stop,
                }
            }
            OrderKind::StopLimit { stop, limit } => {
                if !stop.is_finite() || stop <= 0.0 || !limit.is_finite() || limit <= 0.0 {
                    return Err(MarketError::InvalidPrice);
                }
                let armed = match order.side {
                    Side::Buy => mid >= stop,
                    Side::Sell => mid <= stop,
                };
                armed
                    && match order.side {
                        Side::Buy => mid <= limit,
                        Side::Sell => mid >= limit,
                    }
            }
        };
        if !trigger {
            return Ok(OrderStatus::Pending);
        }
        // Worst-case bounded simulated execution, explicitly not a quote or promise of fill.
        let slip = order.max_slippage_bps as f64 / 10000.0;
        let execution = match order.side {
            Side::Buy => mid * (1.0 + slip),
            Side::Sell => mid * (1.0 - slip),
        };
        if !execution.is_finite() || execution <= 0.0 {
            return Err(MarketError::InvalidPrice);
        }
        let qty = match order.side {
            Side::Buy => self
                .account
                .buy(order.fraction, execution, order.fee_rate)?,
            Side::Sell => {
                let q = self.account.base_qty * order.fraction;
                self.account
                    .sell(order.fraction, execution, order.fee_rate)?;
                q
            }
        };
        Ok(OrderStatus::Filled {
            quantity: qty,
            price: execution,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn order(side: Side, kind: OrderKind) -> PaperOrder {
        PaperOrder {
            side,
            kind,
            fraction: 0.5,
            fee_rate: 0.01,
            max_slippage_bps: 50,
        }
    }
    #[test]
    fn limit_buy_only_if_crossed() {
        let mut e = OrderEngine::new(100.0).unwrap();
        assert_eq!(
            e.submit(order(Side::Buy, OrderKind::Limit { limit: 9.0 }), 10.0),
            Ok(OrderStatus::Pending)
        );
        assert!(matches!(
            e.submit(order(Side::Buy, OrderKind::Limit { limit: 9.0 }), 8.0),
            Ok(OrderStatus::Filled { .. })
        ))
    }
    #[test]
    fn stop_loss_sells_after_trigger() {
        let mut e = OrderEngine::new(100.0).unwrap();
        e.submit(order(Side::Buy, OrderKind::Market), 10.0).unwrap();
        let initial = e.account.base_qty;
        assert_eq!(
            e.submit(order(Side::Sell, OrderKind::Stop { stop: 9.0 }), 10.0),
            Ok(OrderStatus::Pending)
        );
        e.submit(order(Side::Sell, OrderKind::Stop { stop: 9.0 }), 8.0)
            .unwrap();
        assert!(e.account.base_qty < initial)
    }
    #[test]
    fn stop_limit_does_not_force_fill_outside_limit() {
        let mut e = OrderEngine::new(100.0).unwrap();
        e.submit(order(Side::Buy, OrderKind::Market), 10.0).unwrap();
        assert_eq!(
            e.submit(
                order(
                    Side::Sell,
                    OrderKind::StopLimit {
                        stop: 9.0,
                        limit: 8.5
                    }
                ),
                8.0
            ),
            Ok(OrderStatus::Pending)
        )
    }
    #[test]
    fn sizing_zero_is_noop() {
        let mut e = OrderEngine::new(100.0).unwrap();
        let mut o = order(Side::Buy, OrderKind::Market);
        o.fraction = 0.0;
        e.submit(o, 10.0).unwrap();
        assert_eq!(e.account.cash_usd, 100.0)
    }
    #[test]
    fn invalid_slippage_rejected() {
        let mut e = OrderEngine::new(100.0).unwrap();
        let mut o = order(Side::Buy, OrderKind::Market);
        o.max_slippage_bps = 10001;
        assert_eq!(e.submit(o, 10.0), Err(MarketError::InvalidFraction))
    }
}
