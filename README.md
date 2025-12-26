# Agile Tools Suite

CLI agile tools for small teams, designed for AI agent interaction. Each tool is a separate binary that can be installed and billed independently.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│              Individual CLI Tools                        │
│  standup │ bug │ retro │ task │ decision │ note │ etc  │
├─────────────────────────────────────────────────────────┤
│                    agile-core                            │
│         (sync, storage, types, auth)                     │
├──────────┬──────────┬──────────────────────────────────┤
│  Local   │   HTTP   │   Git (planned)                   │
│  Sync    │   Sync   │   Sync                            │
└──────────┴──────────┴──────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────┐
│                   agile-server                           │
│      (REST API, PostgreSQL, JWT auth, teams)            │
└─────────────────────────────────────────────────────────┘
```

## Tools

| Binary | Description | Install |
|--------|-------------|---------|
| `standup` | Daily standups - yesterday, today, blockers | `cargo install --path crates/standup` |
| `bug` | Bug/issue tracking with priority & status | `cargo install --path crates/bug` |
| `retro` | Retrospectives - good, bad, action items | `cargo install --path crates/retro` |
| `task` | Kanban task management | `cargo install --path crates/task` |
| `decision` | Architectural decision records (ADRs) | `cargo install --path crates/decision` |
| `note` | Meeting notes and context | `cargo install --path crates/note` |
| `review` | Code review request tracking | `cargo install --path crates/review` |
| `kudos` | Team appreciation and wins | `cargo install --path crates/kudos` |

## Quick Start

```bash
# Install a tool
cargo install --path crates/standup

# Initialize
standup init

# Use it
standup add -y "Fixed auth bug" -t "Working on API"
standup today --json
```

## Sync

Each tool supports syncing to a backend server:

```bash
# Login to sync service
standup login --email you@example.com --password secret --server https://api.example.com

# Sync changes
standup sync

# Check sync status
standup status
```

## Server

Run the backend server:

```bash
# Set environment variables
export DATABASE_URL=postgres://localhost/agile
export JWT_SECRET=your-secret-key

# Run server
cargo run --bin agile-server
```

### API Endpoints

**Auth:**
- `POST /api/v1/auth/register` - Register new user
- `POST /api/v1/auth/login` - Login
- `POST /api/v1/auth/refresh` - Refresh token
- `GET /api/v1/auth/me` - Get current user

**Teams:**
- `POST /api/v1/teams` - Create team
- `GET /api/v1/teams` - List teams
- `GET /api/v1/teams/:id` - Get team
- `POST /api/v1/teams/:id/members` - Add member
- `GET /api/v1/teams/:id/members` - List members

**Sync:**
- `POST /api/v1/sync/push` - Push local changes
- `GET /api/v1/sync/pull` - Pull remote changes
- `GET /api/v1/sync/version` - Get current version

## JSON Output

All commands support `--json` for agent parsing:

```bash
standup list --json
bug list --json
task board --json
```

## Storage

Data is stored in `.agile/` in YAML format, designed to be git-friendly.

## Development

```bash
# Build all
cargo build

# Run tests
cargo test

# Build release
cargo build --release
```
