FROM lukemathwalker/cargo-chef:latest as chef
WORKDIR /app

FROM chef AS planner
COPY ./backend/ .
RUN cargo chef prepare

FROM chef AS builder
COPY --from=planner /app/recipe.json .
COPY ./backend/types ./types
RUN cargo chef cook --release
COPY ./backend/ .
RUN cargo build --release
RUN mv ./target/release/backend ./app

FROM debian:stable-slim AS runtime
WORKDIR /app
COPY --from=builder /app/app /usr/local/bin/
COPY ./backend/dist ./dist
ENTRYPOINT ["/usr/local/bin/app"]