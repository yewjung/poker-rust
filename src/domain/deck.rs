use eyre::{ensure, Result};
use poker::{Card, Rank, Suit};
use rand::Rng;

use crate::error::Error::{EmptyDeck, InvalidPosition};

// static u64s
const P: u64 = 0x5555555555555555;
const Q: u64 = 0x3333333333333333;
const R: u64 = 0x0f0f0f0f0f0f0f0f;
const S: u64 = 0x00ff00ff00ff00ff;

const MASK_1: u64 = 0xff;
const MASK_2: u64 = 0xf;
const MASK_3: u64 = 0x7;
const MASK_4: u64 = 0x3;
const MASK_5: u64 = 0x1;

#[derive(Debug)]
pub struct Deck(u64);

const FULL_DECK_INT: u64 = 0x000f_ffff_ffff_ffff;

impl Deck {
    pub fn new() -> Self {
        Deck(FULL_DECK_INT)
    }

    pub fn draw(&mut self) -> Result<Card> {
        let ones = self.0.count_ones();
        // TODO: Optimize RNG by creating a pool of generators
        let n = rand::thread_rng().gen_range(1..=ones);

        // Find the position of the nth leading 1 bit
        let position = pos_of_leading_1_bit(n as u64, self.0)? - 1;

        // Flip the nth trailing bit
        self.0 &= !(1 << position);
        Ok(Card::new(i_to_rank(position), i_to_suit(position)))
    }
}

fn i_to_rank(i: u64) -> Rank {
    match i % 13 {
        0 => Rank::Two,
        1 => Rank::Three,
        2 => Rank::Four,
        3 => Rank::Five,
        4 => Rank::Six,
        5 => Rank::Seven,
        6 => Rank::Eight,
        7 => Rank::Nine,
        8 => Rank::Ten,
        9 => Rank::Jack,
        10 => Rank::Queen,
        11 => Rank::King,
        12 => Rank::Ace,
        _ => unreachable!(),
    }
}

fn i_to_suit(i: u64) -> Suit {
    match i / 13 {
        0 => Suit::Spades,
        1 => Suit::Hearts,
        2 => Suit::Diamonds,
        3 => Suit::Clubs,
        _ => unreachable!(),
    }
}
/// Returns the position of the nth (1-indexed) leading 1 bit in v.
/// If v is 0, returns 64.
/// If n is greater than the number of 1 bits in v, returns 64.
/// Position is 1-indexed.
fn pos_of_leading_1_bit(mut r: u64, v: u64) -> Result<u64> {
    ensure!(v > 0, EmptyDeck);
    ensure!(r <= v.count_ones().into(), InvalidPosition(r));
    let a = (v & P) + ((v >> 1) & P);
    let b = (a & Q) + ((a >> 2) & Q);
    let c = (b & R) + ((b >> 4) & R);
    let d = (c & S) + ((c >> 8) & S);

    let mut t = (d >> 32) + (d >> 48);
    t &= (1 << 16) - 1;

    // Now do branchless select!
    let mut s = 64;

    if r > t {
        s -= 32;
        r -= t;
    }
    t = (d >> (s - 16)) & MASK_1;

    if r > t {
        s -= 16;
        r -= t;
    }
    t = (c >> (s - 8)) & MASK_2;

    if r > t {
        s -= 8;
        r -= t;
    }
    t = (b >> (s - 4)) & MASK_3;

    if r > t {
        s -= 4;
        r -= t;
    }
    t = (a >> (s - 2)) & MASK_4;

    if r > t {
        s -= 2;
        r -= t;
    }
    t = (v >> (s - 1)) & MASK_5;

    if r > t {
        s -= 1;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::collections::HashSet;

    #[test]
    fn make_sure_full_deck_int_has_only_52_trailing_1_bits() {
        assert_eq!(52, Deck::new().0.count_ones());
    }

    #[test]
    fn test_pos_of_trailing_1_bit() -> Result<()> {
        let deck: u64 = 0b0111_1100;
        assert_eq!(5, pos_of_leading_1_bit(3, deck)?);
        Ok(())
    }

    #[test]
    fn test_pos_of_trailing_1_bit_with_empty_deck() {
        let deck: u64 = 0;
        let error = pos_of_leading_1_bit(0, deck).unwrap_err();
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
        let all_cards = (0..52)
            .map(|_| deck.draw())
            .collect::<Result<HashSet<_>>>()?;
        assert_eq!(52, all_cards.len());
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
