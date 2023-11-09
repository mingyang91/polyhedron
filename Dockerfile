# Build stage
FROM nvidia/cuda:12.2.2-devel-ubuntu22.04 as builder

# Install python3.11 and build dependencies
RUN apt-get update
RUN apt-get install -y software-properties-common
#RUN add-apt-repository ppa:deadsnakes/ppa

RUN apt-get update
RUN apt-get install -y libssl-dev cmake python3-dev curl pkg-config clang

# install rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s  -- --default-toolchain stable -y
ENV PATH=/root/.cargo/bin:$PATH

# Just copy the manifest files to cache dependencies
COPY Cargo.toml Cargo.lock ./

# Download dependencies
RUN mkdir -p src/bin && echo "fn main() {println!(\"if you see this, the build broke\")}" > src/bin/bigbot.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release

COPY ./src ./src

# Build the project with release profile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release

# Runtime stage
FROM nvidia/cuda:12.2.2-runtime-ubuntu22.04 as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y ca-certificates python3-dev && rm -rf /var/lib/apt/lists/*

# Just copy the binary from the build stage
COPY --from=builder /target/release/polyhedron /usr/local/bin/polyhedron
COPY ./models/ggml-large-encoder.mlmodelc ./models/ggml-large-encoder.mlmodelc
COPY ./models/ggml-large.bin ./models/ggml-large.bin
COPY ./config/dev.yaml ./config/dev.yaml
COPY ./static ./static

# Run the binary
CMD ["polyhedron"]
