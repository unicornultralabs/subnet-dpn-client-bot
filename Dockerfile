FROM rustlang/rust:nightly-bookworm as build

WORKDIR /app

# Install dependencies (combine into one step for efficiency)
RUN apt-get update && \
    apt-get install -y --no-install-recommends gcc llvm clang libtool protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*

# Cache dependencies to speed up builds
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN RUSTFLAGS="-C target-cpu=native" cargo build --release && rm -rf src

# Copy actual source code and build
COPY . .
RUN RUSTFLAGS="-C target-cpu=native" cargo build --release && \
    strip /app/target/release/proxy_client_bot

# Minimal runtime image
FROM ubuntu:22.04

# Install only necessary runtime dependencies
RUN apt-get update && apt-get -y install --no-install-recommends libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=build /app/target/release/proxy_client_bot /usr/local/bin/proxy_client_bot

ENTRYPOINT ["proxy_client_bot"]
