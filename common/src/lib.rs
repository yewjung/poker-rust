use proc_macro::TokenStream;
use quote::quote;
use std::{env, fs};
use std::path::PathBuf;

#[proc_macro]
pub fn generate_image_lookup(_input: TokenStream) -> TokenStream {
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
                    #file_name => Some(#content),
                });
            }
        }
    }

    let expanded = quote! {
        pub fn lookup_image(key: &str) -> Option<&'static str> {
            match key {
                #(#entries)*
                _ => None,
            }
        }
    };

    TokenStream::from(expanded)
}