FROM rust:1.90 AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y \
    pkg-config \
    libasound2-dev \
    libudev-dev \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY default_map.json default_map.json
COPY server/Cargo.toml server/Cargo.toml
COPY client/Cargo.toml client/Cargo.toml
RUN mkdir -p server/src && echo "fn main() {}" > server/src/main.rs
RUN mkdir -p client/src && echo "fn main() {}" > client/src/main.rs
RUN cargo build --release -p spin-snowball-server
RUN rm -rf server/src client/src
COPY server server
COPY client client
RUN cargo build --release -p spin-snowball-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/spin-snowball-server /app/server
COPY --from=builder /app/tdefault_map.json /app/default_map.json
CMD ["./server"]
