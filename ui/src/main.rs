pub use app::App;

pub mod app;
use cli_log::*;
#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    init_cli_log!("poker");
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal).await;
    ratatui::restore();
    result
}
