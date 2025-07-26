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
then, run the command below to start the game:

```bash
ui
```

### Run backend server without Docker
1. start up docker-compose with the following command:
```bash
docker-compose up --build -d
```
2. stop the backend server with:
```bash
docker-compose stop <backend container id>
```
3. run the following command to start the server:
```bash
DATABASE_URL=postgres://user:password@localhost:5432/my_database RUST_LOG=debug cargo run
```