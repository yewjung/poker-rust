# ♠️ Poker TUI – Multiplayer Texas Hold 'Em in the Terminal

A real-time multiplayer **Texas Hold 'Em** poker game built in Rust with a clean, responsive Terminal User Interface (TUI). Challenge your friends right from your terminal window.

## 🎮 Features

- ♠️ Texas Hold 'Em poker rules
- 🔁 Real-time multiplayer support
- 🖥️ Terminal User Interface (TUI) – no GUI required
- 🕹️ Intuitive keyboard controls
- 📡 Built on top of a custom networking backend in Rust

## 🚀 Installation

Make sure you have [Rust](https://www.rust-lang.org/tools/install) installed.

Then, install the game with:

```bash
cargo install --git https://www.github.com/yewjung/poker-rust ui
```

### Run server without Docker
1. Cd into backend directory
2. run the following command to start the server:
```bash
DATABASE_URL=postgres://user:password@localhost:5432/my_database RUST_LOG=debug cargo run
```