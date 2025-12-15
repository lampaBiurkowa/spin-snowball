FROM rust:1.90 AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release -p spin-snowball-server
RUN rm -rf src

COPY . .
RUN cargo build --release -p spin-snowball-server
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libasound2 \
    libudev1 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/server /app/server

CMD ["./server"]
