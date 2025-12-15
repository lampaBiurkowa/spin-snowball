FROM rust:1.90 AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y \
    pkg-config \
    libasound2-dev \
    libudev-dev \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY server/Cargo.toml server/Cargo.toml

RUN mkdir -p server/src \
 && echo "fn main() {}" > server/src/main.rs

RUN cargo build --release -p spin-snowball-server
RUN rm -rf server/src client/src
COPY server server
RUN cargo build --release -p spin-snowball-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/spin-snowball-server /app/server

EXPOSE 9001
CMD ["./server"]
