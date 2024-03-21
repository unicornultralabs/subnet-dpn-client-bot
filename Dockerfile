FROM rustlang/rust:nightly-bookworm as build

WORKDIR /app

# Runtime image
RUN     apt-get update
ARG     DEBIAN_FRONTEND=noninteractive
RUN     apt-get update && \
	apt-get install -y gcc llvm clang libtool && \
	apt-get install -y protobuf-compiler && \
	rm -rf /var/lib/apt/lists/*

# Copy source code & install
COPY    . .
RUN     RUSTFLAGS="-C target-cpu=native" cargo build --release
RUN		cp /app/target/release/proxy_client_bot /usr/local/bin/proxy_client_bot

FROM    ubuntu:22.04
RUN     apt-get update && apt-get -y install libssl3
COPY    --from=build /app/target/release/proxy_client_bot /usr/local/bin/proxy_client_bot
WORKDIR /

ENTRYPOINT     ["proxy_client_bot"]