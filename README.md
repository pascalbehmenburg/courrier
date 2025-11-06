# Courrier

An asynchronous email fetching service built with Rust. Courrier connects to IMAP servers, downloads emails to `.eml` files, and provides a web dashboard for monitoring and managing your emails. It may be used for automated mail backups on a server.

## Features

- **Multi-Account Support**: Fetch emails from multiple IMAP accounts across different servers
- **Automatic Mailbox Discovery**: Automatically discovers and fetches from all mailboxes
- **Incremental Fetching**: Tracks fetched emails to avoid duplicates
- **Web Dashboard**: Provides a dashboard for monitoring fetch status and statistics
- **Periodic Fetching**: Optional automatic fetching at configurable intervals
- **Docker Support**: Ready-to-use Docker container with volume mounts
- **SQLite Database**: Lightweight database for tracking fetched emails
- **Async Architecture**: Built with Tokio for high-performance concurrent operations

## Installation

### Using Cargo

```bash
cargo install --path .
```

Or clone and build:

```bash
git clone <repository-url>
cd mailster
cargo build --release
```

### Using Docker

```bash
docker build -t courrier .
```

## Configuration

Create a `Config.toml` file in the working directory (or mount it at `/config/Config.toml` in Docker):

```toml
# Storage configuration
email_storage_path = "emails"  # Path where emails will be stored

# Fetch configuration
fetch_on_startup = true        # Automatically fetch emails when server starts
fetch_interval_seconds = 3600  # Optional: Automatically fetch every N seconds (e.g., 3600 = 1 hour)

# Example apple IMAP server configuration
[[servers]]
host = "imap.mail.me.com"
port = 993  # Optional, defaults to 993 if not specified
accounts = [
  { email = "your@mail.com", username = "mailer", password = "your-app-specific-password" },
  { email = "other@mail.com", username = "mailer", password = "your-app-specific-password" }
]

# Example gmail IMAP server configuration
[[servers]]
host = "imap.gmail.com"
port = 993
accounts = [
  { email = "your-email@gmail.com", username = "your-email@gmail.com", password = "your-app-specific-password" }
]
```

See `Config.toml.example` for a complete example.

## Usage

### CLI Mode

Run a one-time fetch operation:

```bash
courrier fetch
```

### Server Mode

Start the web dashboard (default):

```bash
courrier server
# or
courrier server 8080  # Custom port
```

The dashboard will be available at `http://localhost:3000` (or your specified port).

### Environment Variables

- `COURRIER_DB_PATH`: Path to the SQLite database file (default: `courrier.db`)

## Docker Usage

### Building the Image

```bash
docker build -t courrier .
```

### Running the Container

```bash
docker run -d \
  --name courrier \
  -p 3000:3000 \
  -v /path/to/your/config:/config \
  -v /path/to/your/data:/data \
  courrier
```

**Volume Mounts:**
- `/config`: Mount your `Config.toml` file here
- `/data`: Emails and database (`courrier.db`) will be stored here

**Example:**

```bash
# Create directories
mkdir -p ~/courrier/config ~/courrier/data

# Copy your Config.toml
cp Config.toml ~/courrier/config/

# Run container
docker run -d \
  --name courrier \
  -p 3000:3000 \
  -v ~/courrier/config:/config \
  -v ~/courrier/data:/data \
  courrier
```

**Important:** Make sure your `Config.toml` has `email_storage_path = "/data"` (or a subdirectory like `/data/emails`) when running in Docker.

### Docker Compose

Create a `docker-compose.yml`:

```yaml
version: '3.8'

services:
  courrier:
    build: .
    container_name: courrier
    ports:
      - "3000:3000"
    volumes:
      - ./config:/config
      - ./data:/data
    restart: unless-stopped
    environment:
      - COURRIER_DB_PATH=/data/courrier.db
```

See `docker-compose.example.yml` for a full example.

Run with:

```bash
docker-compose up -d
```

## Development

### Prerequisites

- Rust (latest stable)
- Nix (optional, for development environment)

### Setup with Nix

The project includes a Nix flake for a reproducible development environment:

```bash
nix develop
```

## API Endpoints

The web dashboard provides the following REST API endpoints:

- `GET /` - Web dashboard (HTML)
- `GET /api/accounts` - List all configured accounts
- `GET /api/stats` - Get statistics (total emails, storage, per-account stats)
- `POST /api/fetch` - Trigger a manual fetch operation
- `GET /api/fetch/status` - Get current fetch operation status

## How It Works

1. **Configuration Loading**: Reads `Config.toml` to get IMAP server and account details
2. **Database Initialization**: Creates/opens SQLite database to track fetched emails
3. **Mailbox Discovery**: Connects to each account and lists all available mailboxes
4. **Incremental Fetching**: For each mailbox, fetches only new emails (not in database)
5. **Email Storage**: Saves emails as `.eml` files organized by account and mailbox
6. **Web Dashboard**: Provides real-time monitoring and manual fetch triggers

## License

MIT License - see [LICENSE](LICENSE) file for details.

Copyright (c) 2025 Pascal Behmenburg

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

For issues, questions, or contributions, please open an issue on the repository.

