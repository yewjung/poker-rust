use artem::{convert, ConfigBuilder};
use poker::Card;
use image::open;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // println!("cargo:rerun-if-changed=build.rs");
    // create folder called text_assets in the root of the project if it doesn't exist
    std::fs::create_dir_all("text_assets")?;
    // convert each image in assets folder to ascii art and save it in text_assets folder
    let image_config = ConfigBuilder::new()
        .target_size(std::num::NonZeroU32::new(35).unwrap())
        .invert(false)
        .build();
    poker::deck::generate()
        .map(Card::rank_suit_string)
        .try_for_each(|card| {
            let image = open(format!("assets/{}.png", card))?;
            let ascii_art = convert(image, &image_config);
            println!("{}: {}", card, ascii_art);
            std::fs::write(format!("text_assets/{}.txt", card), ascii_art)?;
            Ok(())
        })
}