//! The market (`ECONOMY.TXT`, object `ECO` at 0x9d5f28): resources are exchanged at rates
//! shared by all players. Every trade moves the rates of both resources towards a limit, and
//! every tick they drift back towards their normal values. Loader 0x4a8f82,
//! `PerformOperation` 0x4a9d80, `Process` 0x4aa0af. See `docs/simulation.md`.
//!
//! The engine keeps the rates as doubles. So does this module: IEEE double additions,
//! multiplications and divisions give the same results on every platform (Rust never fuses
//! them), and the engine's x87 runs at double precision, so the values match the original
//! except where its 80-bit intermediates round differently.

use std::hash::{Hash, Hasher};

use crate::catalog::resource_index;
use crate::world::World;

/// Resources on the market.
pub const SLOTS: usize = 6;

/// The rate table of `ECONOMY.TXT`: per market slot `RES BUY b0 b1 b2 CHANGE c ex TIMERETURN
/// t SELL s0 s1 s2 CHANGE c' ex' TIMERETURN t'`. Values are read as floats (`%g` into
/// `float`) and widened to doubles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketDef {
    /// Resource index of each slot (`EcResID`).
    pub res: [usize; SLOTS],
    /// Buying price: low limit `b0`, normal `b1`, high limit `b2`.
    pub buy: [[f64; 3]; SLOTS],
    pub sell: [[f64; 3]; SLOTS],
    /// `ExpBUY = ex / (c · 100)`, `ExpSELL` likewise: how fast trades move the rates.
    pub exp_buy: [f64; SLOTS],
    pub exp_sell: [f64; SLOTS],
    /// `TIMERETURN`: drift back per tick.
    pub time_buy: [f64; SLOTS],
    pub time_sell: [f64; SLOTS],
}

impl MarketDef {
    /// Parses `ECONOMY.TXT`: 11 icon names, then six resource blocks.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let text = String::from_utf8_lossy(data);
        let mut t = text.split_whitespace().skip(11);
        let mut d = Self::default();
        let num =
            |t: &mut dyn Iterator<Item = &str>| -> Option<f64> { Some(f64::from(t.next()?.parse::<f32>().ok()?)) };
        for i in 0..SLOTS {
            d.res[i] = resource_index(t.next()?)?;
            let side = |t: &mut dyn Iterator<Item = &str>| -> Option<([f64; 3], f64, f64)> {
                t.next()?; // BUY / SELL
                let v = [num(t)?, num(t)?, num(t)?];
                t.next()?; // CHANGE
                let (c, ex) = (num(t)?, num(t)?);
                t.next()?; // TIMERETURN
                Some((v, ex / (c * 100.0), num(t)?))
            };
            (d.buy[i], d.exp_buy[i], d.time_buy[i]) = side(&mut t)?;
            (d.sell[i], d.exp_sell[i], d.time_sell[i]) = side(&mut t)?;
        }
        Some(d)
    }

    /// The market slot of a resource index.
    pub fn slot(&self, res: usize) -> Option<usize> {
        self.res.iter().position(|&r| r == res)
    }
}

/// The current rates (`Buy`, `Sell`), shared by all players.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Market {
    pub buy: [f64; SLOTS],
    pub sell: [f64; SLOTS],
}

impl Hash for Market {
    fn hash<H: Hasher>(&self, h: &mut H) {
        for v in self.buy.iter().chain(&self.sell) {
            v.to_bits().hash(h);
        }
    }
}

impl Market {
    /// The normal rates (0x4a94eb: `Buy = Buy1`, `Sell = Sell1`).
    pub fn new(def: &MarketDef) -> Self {
        Self { buy: def.buy.map(|b| b[1]), sell: def.sell.map(|s| s[1]) }
    }

    /// `Process` (0x4aa0af), every tick: each rate above its normal value drops by
    /// `TIMERETURN`, then each below it rises, sell before buy.
    pub fn process(&mut self, def: &MarketDef) {
        for i in 0..SLOTS {
            if self.sell[i] > def.sell[i][1] {
                self.sell[i] -= def.time_sell[i];
            }
            if self.buy[i] > def.buy[i][1] {
                self.buy[i] -= def.time_buy[i];
            }
            if self.sell[i] < def.sell[i][1] {
                self.sell[i] += def.time_sell[i];
            }
            if self.buy[i] < def.buy[i][1] {
                self.buy[i] += def.time_buy[i];
            }
        }
    }

    /// What `amount` of slot `sell` buys of slot `buy` now (truncated, `_ftol`).
    pub fn quote(&self, sell: usize, buy: usize, amount: i32) -> i32 {
        (f64::from(amount) * self.sell[sell] / self.buy[buy]) as i32
    }
}

/// `PerformOperation` (0x4a9d80): `player` sells `amount` (at most its stock) of resource
/// `sell` for resource `buy`. It gets `trunc(amount · Sell[sell] / Buy[buy])`; then the
/// bought resource's rates move towards their high limits by `V = got · ExpBUY` (`r = (r +
/// limit · V) / (1 + V)`) and the sold resource's towards their low limits by `V = amount ·
/// ExpSELL`.
pub fn trade(w: &mut World, player: u8, sell: usize, buy: usize, amount: i32) {
    let Some(def) = w.catalog.market.as_ref() else { return };
    let (Some(s), Some(b)) = (def.slot(sell), def.slot(buy)) else { return };
    if amount <= 0 {
        return;
    }
    let p = &mut w.players[usize::from(player)];
    let amount = amount.min(p.resources[sell]);
    let got = w.market.quote(s, b, amount);
    p.resources[sell] -= amount;
    p.resources[buy] = p.resources[buy].wrapping_add(got);
    let m = &mut w.market;
    let v = f64::from(got) * def.exp_buy[b];
    m.buy[b] = (v * def.buy[b][2] + m.buy[b]) / (1.0 + v);
    m.sell[b] = (v * def.sell[b][2] + m.sell[b]) / (1.0 + v);
    let v = f64::from(amount) * def.exp_sell[s];
    m.sell[s] = (v * def.sell[s][0] + m.sell[s]) / (1.0 + v);
    m.buy[s] = (v * def.buy[s][0] + m.buy[s]) / (1.0 + v);
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const ECONOMY: &[u8] = b"a b c d e f g h i j k\r\nFOOD\r\nBUY   20  25   40   CHANGE  100 1.0 TIMERETURN 0.0001\r\n\
        SELL  5   15   20   CHANGE  100 1.0 TIMERETURN 0.0001\r\nIRON\r\nBUY 140 190 240 CHANGE 100 1.0 TIMERETURN 0.0001\r\n\
        SELL 80 110 140 CHANGE 100 1.0 TIMERETURN 0.0001\r\nWOOD\r\nBUY 40 50 60 CHANGE 100 1.0 TIMERETURN 0.0001\r\n\
        SELL 20 30 40 CHANGE 100 1.0 TIMERETURN 0.0001\r\nCOAL\r\nBUY 140 190 240 CHANGE 100 1.0 TIMERETURN 0.0001\r\n\
        SELL 80 110 140 CHANGE 100 1.0 TIMERETURN 0.0001\r\nSTONE\r\nBUY 75 90 105 CHANGE 100 1.0 TIMERETURN 0.0001\r\n\
        SELL 30 45 60 CHANGE 100 1.0 TIMERETURN 0.0001\r\nGOLD\r\nBUY 270 300 330 CHANGE 100 1.0 TIMERETURN 0.0001\r\n\
        SELL 180 210 240 CHANGE 100 1.0 TIMERETURN 0.0001\r\n";

    #[test]
    fn parses_the_rate_table() {
        let d = MarketDef::parse(ECONOMY).unwrap();
        assert_eq!(d.res, [3, 4, 0, 5, 2, 1]);
        assert_eq!(d.buy[5], [270.0, 300.0, 330.0]);
        assert_eq!(d.exp_sell[0], 1e-4);
        assert_eq!(d.time_buy[0], f64::from(0.0001f32));
        let m = Market::new(&d);
        // 100 wood (sell 30) buy gold at 300: 10.
        assert_eq!(m.quote(d.slot(0).unwrap(), d.slot(1).unwrap(), 100), 10);
    }
}
