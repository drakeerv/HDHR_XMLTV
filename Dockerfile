# Build stage
FROM rust:bookworm AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/hdhr-xmltv /usr/local/bin/hdhr-xmltv

# Create output directory
RUN mkdir -p /output

# Default environment variables
# Supports HDHR_HOST or HDHR_IP for host configuration
# Supports TIMEZONE or TZ for timezone configuration
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
