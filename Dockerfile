FROM rust:alpine AS builder

RUN apk add --no-cache \
    musl-dev \
    protobuf \
    protobuf-dev

WORKDIR /app

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > build.rs

# cache dependencies
RUN cargo build --release

COPY . .
RUN touch src/main.rs build.rs

RUN cargo build --release


FROM alpine:latest

WORKDIR /app

COPY --from=builder /app/target/release/lally .

ENTRYPOINT ["./lally"]
