pub use app::App;

pub mod app;
mod screen_data;

use cli_log::*;
use common::txt_to_hashmap;
use keyring::Entry;
use lazy_static::lazy_static;

txt_to_hashmap!();

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

#[cfg(test)]
mod tests {
    use crate::IMAGE_CACHE;
    use std::fs;

    #[test]
    fn test_txt_to_hashmap() {
        let path = "text_assets";
        assert!(fs::read_dir(path).is_ok());

        assert!(IMAGE_CACHE.len() > 0);
    }
}
