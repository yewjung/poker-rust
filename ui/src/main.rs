pub use app::App;
use std::num::NonZeroU32;

pub mod app;
mod screen_data;

use artem::{convert, ConfigBuilder};
use cli_log::*;
use dashmap::DashMap;
use keyring::Entry;
use lazy_static::lazy_static;
use poker::{deck, Card};
use std::sync::Arc;

lazy_static! {
    static ref TOKEN_MANAGER: Entry =
        Entry::new("poker", "token").expect("Failed to create token manager");
    static ref IMAGE_CACHE: Arc<DashMap<String, String>> = Arc::new(DashMap::new());
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    init_cli_log!("poker");
    color_eyre::install()?;
    init_image_cache()?;
    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}

fn init_image_cache() -> color_eyre::Result<()> {
    let image_config = ConfigBuilder::new()
        .target_size(NonZeroU32::new(35).unwrap())
        .invert(false)
        .build();
    deck::generate()
        .map(Card::rank_suit_string)
        .try_for_each(|card| {
            let image = image::open(format!("assets/{}.png", card))?;
            let ascii_art = convert(image, &image_config);
            IMAGE_CACHE.insert(card.to_string(), ascii_art);
            Ok(())
        })
}
