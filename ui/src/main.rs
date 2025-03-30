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
use std::thread;
use rodio::{Decoder, OutputStream, Sink};
use std::io::Cursor;

generate_image_lookup!();

lazy_static! {
    static ref TOKEN_MANAGER: Entry =
        Entry::new("poker", "token").expect("Failed to create token manager");
}

static DING_SOUND: &[u8] = include_bytes!("../sound_assets/ding.wav");
static CHIPS_SOUND: &[u8] = include_bytes!("../sound_assets/chips.wav");
static CHECK_SOUND: &[u8] = include_bytes!("../sound_assets/check.mp3");

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    init_cli_log!("poker");
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().await?.run(terminal).await;
    ratatui::restore();
    result
}

fn play_sound(bytes: &'static [u8]) {
    thread::spawn(move || {
        // _stream must live as long as the sink
        if let Ok((_stream, stream_handle)) = OutputStream::try_default() {
            if let Ok(sink) = Sink::try_new(&stream_handle) {
                if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                    sink.append(source);
                    sink.sleep_until_end();
                }
            }
        }
    });
}