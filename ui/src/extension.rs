use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub trait Splittable {
    fn split_equal<const N: usize>(area: Rect, direction: Direction) -> [Rect; N];
}

impl Splittable for Layout {
    fn split_equal<const N: usize>(area: Rect, direction: Direction) -> [Rect; N] {
        let n = N as u32;
        match direction {
            Direction::Horizontal => {
                Self::horizontal(Constraint::from_ratios([(1, n); N])).areas(area)
            }
            Direction::Vertical => Self::vertical(Constraint::from_ratios([(1, n); N])).areas(area),
        }
    }
}
