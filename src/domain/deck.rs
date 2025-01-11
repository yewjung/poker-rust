use rand::Rng;

pub struct Deck(u64);

const FULL_DECK_INT: u64 = 0x000f_ffff_ffff_ffff;

impl Deck {
    pub fn new() -> Self {
        Deck(FULL_DECK_INT)
    }

    pub fn draw(&mut self) -> Option<usize> {
        let ones = self.0.count_ones();
        // TODO: Optimize RNG by creating a pool of generators
        let n = rand::thread_rng().gen_range(0..ones);

        // Flip the nth trailing bit
        todo!()

    }
}

fn pos_of_trailing_1_bit(mut n: u64, deck: u64) -> u32 {
    println!("deck: {:08b}", deck);
    let a: u64 = (deck & 0x5555_5555_5555_5555) + ((deck >> 1) & 0x5555_5555_5555_5555);
    let b: u64 = (a & 0x3333_3333_3333_3333) + ((a >> 2) & 0x3333_3333_3333_3333);
    let c: u64 = (b & 0x0f0f_0f0f_0f0f_0f0f) + ((b >> 4) & 0x0f0f_0f0f_0f0f_0f0f);
    let d: u64 = (c & 0x00ff_00ff_00ff_00ff) + ((c >> 8) & 0x00ff_00ff_00ff_00ff);

    let mut pos = 0;
    let mut len_of_window = 32;
    for count in &[d, c, b, a] {
        println!("counts: {:08b}", count);
        let shifted_counts = count.rotate_right(pos);
        println!("shifted_counts: {:08b}", shifted_counts);
        let mask = (1 << len_of_window) - 1;
        println!("mask: {:064b}", mask);
        let s = shifted_counts & mask;
        println!("{}", s);
        println!();
        if n > s {
            pos += len_of_window;
            n -= s;
        }
        len_of_window /= 2;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_sure_full_deck_int_has_only_52_trailing_1_bits() {
        assert_eq!(52, Deck::new().0.count_ones());
    }

    #[test]
    fn test_pos_of_trailing_1_bit() {
        let deck: u64 = 0b0111_1100;
        assert_eq!(5, pos_of_trailing_1_bit(3, deck));
    }
}