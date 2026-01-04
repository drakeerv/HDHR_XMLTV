# Build stage
FROM rust:1.92-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM alpine:latest

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /app

COPY --from=builder /app/target/release/hdhr-xmltv /usr/local/bin/hdhr-xmltv

# Create output directory
RUN mkdir -p /output

# Default environment variables
ENV HDHR_HOST=hdhomerun.local
ENV DAYS=7
ENV HOURS=3
ENV OUTPUT_DIR=/output
ENV OUTPUT_FILE=epg.xml
ENV INTERVAL=0
ENV TZ=UTC
ENV RUST_LOG=info

VOLUME ["/output"]

CMD ["/usr/local/bin/hdhr-xmltv"]
