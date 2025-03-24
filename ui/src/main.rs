pub use app::App;

pub mod app;
mod screen_data;

use cli_log::*;
use keyring::Entry;
use lazy_static::lazy_static;
use common::txt_to_hashmap;

txt_to_hashmap!();

lazy_static! {
    static ref TOKEN_MANAGER: Entry =
        Entry::new("poker", "token").expect("Failed to create token manager");
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    init_cli_log!("poker");
    debug!("len: {}", IMAGE_CACHE.len());
    for key in IMAGE_CACHE.keys() {
        debug!("{}", key);
    }
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use crate::IMAGE_CACHE;

    #[test]
    fn test_txt_to_hashmap() {
        let path = "text_assets";
        assert!(fs::read_dir(path).is_ok());

        assert!(IMAGE_CACHE.len() > 0);
    }
}