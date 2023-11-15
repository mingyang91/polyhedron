FROM nvidia/cuda:11.4.3-cudnn8-devel-ubuntu20.04 as chef
RUN apt-get update && apt-get install -y cmake g++ libclang-dev libssl-dev pkg-config python3-dev curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain stable -y
ENV PATH=/root/.cargo/bin:$PATH
RUN cargo install cargo-chef
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef as builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release

FROM nvidia/cuda:11.4.3-cudnn8-runtime-ubuntu20.04 as runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/polyhedron .
ENTRYPOINT ["./polyhedron"]
