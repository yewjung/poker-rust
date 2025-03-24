use proc_macro::TokenStream;
use quote::quote;
use std::{env, fs};
use std::path::PathBuf;
use cli_log::*;

#[proc_macro]
pub fn txt_to_hashmap(_input: TokenStream) -> TokenStream {
    let path = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map(|path| path.join("text_assets"))
        .expect("Missing `CARGO_MANIFEST_DIR`");
    let mut entries = Vec::new();

    if let Ok(dir) = fs::read_dir(&path) {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "txt") {
                let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
                let content = fs::read_to_string(&path)
                    .expect("Failed to read file");

                entries.push(quote! {
                    (#file_name.to_string(), #content.to_string())
                });
            }
        }
    }

    let output = quote! {
        lazy_static::lazy_static! {
            pub static ref IMAGE_CACHE: std::collections::HashMap<String, String> = {
                let mut map = std::collections::HashMap::from([
                    #(#entries),*
                ]);
                map
            };
        }
    };

    output.into()
}