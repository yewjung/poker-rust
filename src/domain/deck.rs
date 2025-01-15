use rand::Rng;
use eyre::{ensure, Result};

use crate::error::Error::{EmptyDeck, InvalidPosition};

pub struct Deck(u64);

const FULL_DECK_INT: u64 = 0x000f_ffff_ffff_ffff;

impl Deck {
    pub fn new() -> Self {
        Deck(FULL_DECK_INT)
    }

    pub fn draw(&mut self) -> Result<u64> {
        let ones = self.0.count_ones();
        // TODO: Optimize RNG by creating a pool of generators
        let n = rand::thread_rng().gen_range(1..=ones);

        // Find the position of the nth leading 1 bit
        let position = pos_of_leading_1_bit(n as u64, self.0)? - 1;

        // Flip the nth trailing bit
        self.0 &= !(1 << position);
        Ok(position)
    }
}
/// Returns the position of the nth (1-indexed) leading 1 bit in v.
/// If v is 0, returns 64.
/// If n is greater than the number of 1 bits in v, returns 64.
/// Position is 1-indexed.
fn pos_of_leading_1_bit(mut r: u64, v: u64) -> Result<u64> {
    ensure!(v > 0, EmptyDeck);
    ensure!(r <= v.count_ones().into(), InvalidPosition(r));
    let a = (v & 0x5555555555555555) + ((v >> 1) & 0x5555555555555555);
    let b = (a & 0x3333333333333333) + ((a >> 2) & 0x3333333333333333);
    let c = (b & 0x0f0f0f0f0f0f0f0f) + ((b >> 4) & 0x0f0f0f0f0f0f0f0f);
    let d = (c & 0x00ff00ff00ff00ff) + ((c >> 8) & 0x00ff00ff00ff00ff);

    let mut t = (d >> 32) + (d >> 48);
    t &= (1 << 16) - 1;

    // Now do branchless select!
    let mut s = 64;

    if r > t {
        s -= 32;
        r -= t;
    }
    t = (d >> (s - 16)) & 0xff;

    if r > t {
        s -= 16;
        r -= t;
    }
    t = (c >> (s - 8)) & 0xf;

    if r > t {
        s -= 8;
        r -= t;
    }
    t = (b >> (s - 4)) & 0x7;

    if r > t {
        s -= 4;
        r -= t;
    }
    t = (a >> (s - 2)) & 0x3;

    if r > t {
        s -= 2;
        r -= t;
    }
    t  = (v >> (s - 1)) & 0x1;

    if r > t {
        s -= 1;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use crate::error::Error;
    use super::*;

    #[test]
    fn make_sure_full_deck_int_has_only_52_trailing_1_bits() {
        assert_eq!(52, Deck::new().0.count_ones());
    }

    #[test]
    fn test_pos_of_trailing_1_bit() -> Result<()>{
        let deck: u64 = 0b0111_1100;
        assert_eq!(5, pos_of_leading_1_bit(3, deck)?);
        Ok(())
    }

    #[test]
    fn test_pos_of_trailing_1_bit_with_empty_deck() {
        let deck: u64 = 0;
        let error =  pos_of_leading_1_bit(0, deck).unwrap_err();
        let error = error.downcast_ref::<Error>();
        assert!(matches!(error, Some(Error::EmptyDeck)));
    }
    #[test]
    fn test_pos_of_trailing_1_bit_with_rank_greater_than_available_bits() {
        let deck: u64 = 0b0000_0001;
        let error = pos_of_leading_1_bit(2, deck).unwrap_err();
        let error = error.downcast_ref::<Error>();
        assert!(matches!(error, Some(Error::InvalidPosition(2))));
    }

    #[test]
    fn test_draw() -> Result<()> {
        let mut deck = Deck::new();
        let all_cards = (0..52).map(|_| deck.draw()).collect::<Result<HashSet<_>>>()?;
        assert_eq!(52, all_cards.len());
        assert_eq!(all_cards.symmetric_difference(&(0..52).collect::<HashSet<_>>()).count(), 0);
        assert_eq!(deck.0, 0);

        Ok(())
    }

    #[test]
    fn test_pos_of_leading_1_bit_for_all_rank_in_full_deck() -> Result<()> {
        let deck: u64 = 0x000f_ffff_ffff_ffff;
        for i in 1..53 {
            assert_eq!(53 - i, pos_of_leading_1_bit(i, deck)?);
        }
        Ok(())
    }
}