# Build stage
FROM rust:bookworm as builder

# Install python3.11 and build dependencies
RUN apt-get update
RUN apt-get install -y software-properties-common clang cmake
RUN rustup component add rustfmt

# Just copy the manifest files to cache dependencies
COPY Cargo.toml Cargo.lock ./

# Download dependencies
RUN mkdir -p src/bin && echo "fn main() {println!(\"if you see this, the build broke\")}" > src/bin/bigbot.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release

COPY ./src ./src

## Build the project with release profile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim as runtime

# Just copy the binary from the build stage
COPY --from=builder /target/release/polyhedron /usr/local/bin/polyhedron

# Run the binary
CMD ["polyhedron"]
