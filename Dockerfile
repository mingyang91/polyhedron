FROM rust:slim as chef
RUN apt-get update && apt-get install -y curl
RUN cargo install cargo-chef
WORKDIR /app

FROM chef as planner
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM chef as builder
RUN apt-get update && apt-get install -y cmake g++ libclang-dev libssl-dev pkg-config python3-dev
COPY --from=planner /app/recipe.json recipe.json
COPY . .
RUN cargo chef cook --release --recipe-path recipe.json
RUN cargo build --release

FROM debian:bookworm-slim as runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/polyhedron .
ENTRYPOINT ["./polyhedron"]
