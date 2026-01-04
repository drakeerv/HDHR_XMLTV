# HDHR_XMLTV

A Rust-based HDHomeRun EPG to XMLTV converter designed to run in a Podman/Docker container. This application fetches Electronic Program Guide (EPG) data from HDHomeRun devices and converts it to XMLTV format for use with media servers like Jellyfin, Plex, etc.

## Features

- **Async/Efficient**: Built with Tokio and Reqwest for high-performance async operations
- **Simple Configuration**: Environment variable-based configuration
- **Containerized**: Ready-to-use Podman/Docker container
- **Periodic Updates**: Optional interval-based automatic updates
- **Timezone Support**: Configurable timezone for program times

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `HDHR_HOST` or `HDHR_IP` | `hdhomerun.local` | IP address or hostname of your HDHomeRun device |
| `DAYS` | `7` | Number of days of EPG data to fetch (max ~14) |
| `HOURS` | `3` | Number of hours per guide iteration |
| `OUTPUT_DIR` | `/output` | Directory where the XMLTV file will be saved |
| `OUTPUT_FILE` | `epg.xml` | Name of the output XMLTV file |
| `INTERVAL` | `0` | Update interval in seconds (0 = run once and exit) |
| `TIMEZONE` or `TZ` | `UTC` | Timezone for program times (e.g., `America/New_York`, `Europe/London`) |
| `RUST_LOG` | `info` | Log level (`error`, `warn`, `info`, `debug`, `trace`) |

> **Note**: The application supports both `HDHR_HOST`/`HDHR_IP` and `TIMEZONE`/`TZ` for compatibility with existing configurations.

## Usage

### Using Podman

#### One-time Run
```bash
podman run --rm \
  -v ./xmltv:/output:Z \
  -e HDHR_HOST=192.168.1.100 \
  -e DAYS=7 \
  ghcr.io/drakeerv/hdhr-xmltv:latest
```

#### Periodic Updates (runs every 12 hours)
```bash
podman run -d \
  --name hdhr-xmltv \
  -v ./xmltv:/output:Z \
  -e HDHR_HOST=192.168.1.100 \
  -e DAYS=7 \
  -e INTERVAL=43200 \
  -e TIMEZONE=America/New_York \
  ghcr.io/drakeerv/hdhr-xmltv:latest
```

### Using Docker

#### One-time Run
```bash
docker run --rm \
  -v ./xmltv:/output \
  -e HDHR_HOST=192.168.1.100 \
  -e DAYS=7 \
  ghcr.io/drakeerv/hdhr-xmltv:latest
```

#### Periodic Updates (runs every 12 hours)
```bash
docker run -d \
  --name hdhr-xmltv \
  -v ./xmltv:/output \
  -e HDHR_HOST=192.168.1.100 \
  -e DAYS=7 \
  -e INTERVAL=43200 \
  -e TIMEZONE=America/New_York \
  ghcr.io/drakeerv/hdhr-xmltv:latest
```

### Building from Source

#### Local Build and Run
```bash
cargo build --release
HDHR_HOST=192.168.1.100 ./target/release/hdhr-xmltv
```

#### Build Container with Podman
```bash
podman build -t hdhr-xmltv .
```

#### Build Container with Docker
```bash
docker build -t hdhr-xmltv .
```

## Output

The application generates an XMLTV-formatted file (default: `epg.xml`) in the output directory. This file can be used with:

- **Jellyfin**: Settings → Live TV → EPG → XMLTV path
- **Plex**: Add as XMLTV Guide Data source
- **TVHeadend**: Configuration → EPG Grabber → External XMLTV
- Any other application that supports XMLTV format

## Example Docker Compose

### Simple Setup (Run every 4 hours)
```yaml
version: '3.8'

services:
  hdhr-xmltv:
    image: ghcr.io/drakeerv/hdhr-xmltv:latest
    container_name: hdhr-xmltv
    security_opt:
      - label=disable
    volumes:
      - ./xmltv:/output:Z
    environment:
      - HDHR_IP=192.168.10.15
      - TZ=America/New_York
      - OUTPUT_FILE=hdhomerun.xml
      - DAYS=1
      - INTERVAL=14400  # 4 hours
      - RUST_LOG=info
    restart: always
```

### Advanced Setup (Run every 12 hours with more days)
```yaml
version: '3.8'

services:
  hdhr-xmltv:
    image: ghcr.io/drakeerv/hdhr-xmltv:latest
    container_name: hdhr-xmltv
    environment:
      - HDHR_HOST=192.168.1.100
      - DAYS=7
      - HOURS=3
      - INTERVAL=43200  # 12 hours
      - TIMEZONE=America/New_York
      - RUST_LOG=info
    volumes:
      - ./xmltv:/output
    restart: unless-stopped
```

## Migrating from Python Script

If you're currently using the Python-based HDHomeRunEPG_To_XmlTv.py script, here's how to migrate:

### Before (Python):
```yaml
hdhr-xmltv:
  image: python:3-alpine
  container_name: hdhr-xmltv
  security_opt:
    - label=disable
  volumes:
    - ./xmltv:/output:Z
  environment:
    - HDHR_IP=192.168.10.15
    - TZ=America/New_York
  command: >
    sh -c "
    apk add --no-cache tzdata &&
    pip install requests pytz tzlocal &&
    wget -O /app_script.py https://raw.githubusercontent.com/IncubusVictim/HDHomeRunEPG-to-XmlTv/main/HDHomeRunEPG_To_XmlTv.py &&
    while true; do
      echo 'Starting EPG Update...' &&
      python /app_script.py --host $$HDHR_IP --filename /output/hdhomerun.xml --days 1 &&
      echo 'Update Complete. Sleeping for 4 hours...' &&
      sleep 14400;
    done"
  restart: always
```

### After (Rust):
```yaml
hdhr-xmltv:
  image: ghcr.io/drakeerv/hdhr-xmltv:latest
  container_name: hdhr-xmltv
  security_opt:
    - label=disable
  volumes:
    - ./xmltv:/output:Z
  environment:
    - HDHR_IP=192.168.10.15
    - TZ=America/New_York
    - OUTPUT_FILE=hdhomerun.xml
    - DAYS=1
    - INTERVAL=14400  # 4 hours
  restart: always
```

### Benefits of Migration:
- **Smaller image**: ~20MB vs ~150MB for Python Alpine with dependencies
- **Faster startup**: No need to download scripts or install dependencies
- **Lower memory usage**: Rust's efficiency means less resource consumption
- **Better performance**: Async operations with Tokio for faster EPG fetching
- **Built-in loop**: No need for shell scripts with `while true` loops

## Logging

The application uses structured logging with the following levels:
- `error`: Critical errors only
- `warn`: Warnings and errors
- `info`: General information, warnings, and errors (default)
- `debug`: Detailed debugging information
- `trace`: Very verbose trace-level logging

Set the log level using the `RUST_LOG` environment variable.

## Troubleshooting

### Cannot find HDHomeRun device
- Ensure your HDHomeRun is on the same network
- Try using the IP address instead of hostname in `HDHR_HOST`
- Check that your HDHomeRun device is powered on and functioning

### Empty or incomplete EPG data
- Check your HDHomeRun subscription status
- Verify the device has an active internet connection
- Try reducing the `DAYS` value (max is around 14 days)

### Permission denied writing to output
- Ensure the output directory has correct permissions
- When using Podman with SELinux, use the `:Z` flag on volume mounts

## License

GPL - See LICENSE file for details

## Credits

Based on the original Python implementation by Incubus Victim: [HDHomeRunEPG-to-XmlTv](https://github.com/IncubusVictim/HDHomeRunEPG-to-XmlTv)
