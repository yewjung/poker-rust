pub use app::App;

pub mod app;
mod data;
mod extension;
mod game;
mod lobby;
mod login;

use cli_log::*;
use common::generate_image_lookup;
use keyring::Entry;
use lazy_static::lazy_static;

generate_image_lookup!();

lazy_static! {
    static ref TOKEN_MANAGER: Entry =
        Entry::new("poker", "token").expect("Failed to create token manager");
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    init_cli_log!("poker");
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}
