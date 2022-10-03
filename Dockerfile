FROM rust:1-slim as planner
WORKDIR /usr/src/wacr
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare  --recipe-path recipe.json

FROM rust:1-slim as cacher
WORKDIR /usr/src/wacr
RUN apt update -y && apt -y install pkg-config
RUN cargo install cargo-chef
COPY --from=planner /usr/src/wacr/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM rust:1-slim as builder
WORKDIR /usr/src/wacr
RUN apt update -y && apt -y install pkg-config
COPY . .
COPY --from=cacher /usr/src/wacr/target target
COPY --from=cacher $CARGO_HOME $CARGO_HOME
RUN cargo build --release --bin wacr

FROM debian:buster-slim
COPY --from=builder /usr/src/wacr/target/release/wacr /usr/local/bin/wacr
ENV JWT_EXPIRATION=3600
ENV GARBAGE_COLLECTOR_TTL=3600
ENV AUDIO_PATH=/tmp
CMD ["wacr"]