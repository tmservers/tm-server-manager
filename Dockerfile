FROM rust AS builder
WORKDIR /tm-server-bridge
COPY . .

ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN apt-get update
RUN apt-get -y install zstd libssl-dev lld g++ clang pkg-config openssl
RUN cargo build -p tm-server-bridge --release --target x86_64-unknown-linux-gnu

FROM scratch
COPY --from=builder /tm-server-bridge/target/x86_64-unknown-linux-gnu/release/tm-server-bridge /tm-server-bridge
ENTRYPOINT ["./tm-server-bridge"]