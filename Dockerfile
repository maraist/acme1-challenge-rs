# Build stage
FROM rust:bookworm as builder
RUN apt-get update && apt install -y openssl zip llvm time build-essential gzip sysstat pkg-config cmake libssl-dev \
    && rm -rf /var/lib/apt/lists/* && apt-get clean
RUN rustup default nightly
WORKDIR /usr/src/app
# Build dependencies
RUN mkdir ws && mkdir recallai1
COPY ./Cargo.* ./
COPY ./README.md ./
COPY ./web-server/ web-server/
COPY ./recallai/Cargo* recallai/
COPY ./recallai/src/ recallai/src/
RUN cargo build --release
# Run stage
FROM debian:bookworm-slim as run
RUN apt-get update && apt install -y openssl zip llvm time build-essential gzip sysstat pkg-config cmake \
    && rm -rf /var/lib/apt/lists/* && apt-get clean
COPY --from=builder /usr/src/app/target/release/recallai-ws /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app", "--port", "8080", "--num-threads", "6", "--base-url", "http://localhost:8080"]
EXPOSE 8080
