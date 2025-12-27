# What Actually Works Today

This is an honest assessment of what you can use right now.

## TL;DR

**Works great:** All 8 CLI tools for local use with `--json` output for AI agents.
**Works:** Server with auth, teams, HTTP sync push, search.
**Broken:** Git sync pull, incremental sync, some CRUD gaps.

---

## CLI Tools - All Work Locally

Every tool follows the same pattern and stores data in `.agile/` as YAML files.

### standup - Fully Working

```bash
cargo install --path crates/standup

standup init                                    # Initialize in current project
standup add -y "Fixed auth bug" -t "API refactor" -b "Deployment issues"
standup add -y "Shipped feature X"              # Minimal entry
standup list                                    # Show all
standup list --since 2024-01-01                 # Filter by date
standup today                                   # Today's entries only
standup list --json                             # JSON for AI agents
```

### bug - Fully Working

```bash
cargo install --path crates/bug

bug init
bug new "Login fails on Safari" --priority high --label browser --label auth
bug list                                        # All bugs
bug list --status open --priority high          # Filtered
bug show BUG-1                                  # View details
bug update BUG-1 --status in_progress --assignee alice
bug close BUG-1                                 # Mark resolved
bug list --json
```

### task - Working (minor gaps)

```bash
cargo install --path crates/task

task init
task add "Implement caching layer" --assignee bob
task list
task start TASK-1                               # Move to in_progress
task done TASK-1                                # Mark complete
task assign TASK-1 alice                        # Reassign
task move TASK-1 blocked                        # Change status
task board                                      # Shows counts only (not visual board)
task list --json
```

Missing: `task show TASK-1` to view individual task details.

### retro - Working (minor gaps)

```bash
cargo install --path crates/retro

retro init
retro good "Fast deployment pipeline"          # What went well
retro bad "Too many meetings"                  # What didn't
retro action "Reduce standup to 10 min" --owner alice  # Action items
retro list                                     # All items
retro done RETRO-3                             # Complete action item
retro list --json
```

Missing: Can't view or update individual items.

### decision - Fully Working

```bash
cargo install --path crates/decision

decision init
decision record "Use PostgreSQL for primary DB" \
  --context "Need ACID compliance" \
  --alternatives "MongoDB, MySQL" \
  --consequences "Requires SQL expertise"
decision list
decision list --status accepted
decision show DEC-1
decision accept DEC-1
decision deprecate DEC-1 --reason "Migrating to distributed DB"
decision list --json
```

### note - Fully Working

```bash
cargo install --path crates/note

note init
note add "API Design Notes" --content "Use REST for public, gRPC internal"
note add "Meeting Notes" --tag meeting --tag q1
note list
note list --tag meeting                        # Filter by tag
note list --search "API"                       # Search content
note show NOTE-1
note append NOTE-1 "Additional thoughts..."
note delete NOTE-1
note list --json
```

### review - Fully Working

```bash
cargo install --path crates/review

review init
review request feature/user-auth \
  --title "Add OAuth support" \
  --description "Implements Google/GitHub OAuth"
review list
review list --status pending
review show REV-1
review approve REV-1
review request-changes REV-1 --feedback "Need tests"
review comment REV-1 "Looks good overall"
review merge REV-1
review close REV-1                             # Close without merge
review list --json
```

### kudos - Fully Working

```bash
cargo install --path crates/kudos

kudos init
kudos give alice "Amazing debugging session!" --tag teamwork
kudos list
kudos list --to alice                          # Kudos received by alice
kudos list --from bob                          # Kudos given by bob
kudos leaderboard                              # Top recipients
kudos list --json
```

---

## Server - Mostly Working

Requires PostgreSQL and environment variables:

```bash
export DATABASE_URL=postgres://user:pass@localhost/agile
export JWT_SECRET=your-secret-key-here

cargo run --bin agile-server
# Server runs on http://localhost:3000
```

### Auth - Fully Working

```bash
# Register
curl -X POST http://localhost:3000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"you@example.com","password":"secure123","name":"Your Name"}'

# Login (returns access_token and refresh_token)
curl -X POST http://localhost:3000/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"you@example.com","password":"secure123"}'

# Get current user
curl http://localhost:3000/api/v1/auth/me \
  -H "Authorization: Bearer <access_token>"

# Refresh token
curl -X POST http://localhost:3000/api/v1/auth/refresh \
  -H "Content-Type: application/json" \
  -d '{"refresh_token":"<refresh_token>"}'
```

### Teams - Fully Working

```bash
# Create team
curl -X POST http://localhost:3000/api/v1/teams \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"name":"Backend Team","description":"API developers"}'

# List your teams
curl http://localhost:3000/api/v1/teams \
  -H "Authorization: Bearer <token>"

# Add member
curl -X POST http://localhost:3000/api/v1/teams/<team_id>/members \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"user_id":"<user_uuid>","role":"member"}'
```

### Sync - Push Works, Pull Has Issues

```bash
# Push changes (WORKS)
curl -X POST http://localhost:3000/api/v1/sync/push \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"changes":[...]}'

# Pull changes (WORKS but version tracking is broken)
curl "http://localhost:3000/api/v1/sync/pull?entity_type=standup&since_version=0" \
  -H "Authorization: Bearer <token>"
```

### Search - Fully Working

```bash
# Search all entities
curl "http://localhost:3000/api/v1/search?q=authentication" \
  -H "Authorization: Bearer <token>"

# Search specific type
curl "http://localhost:3000/api/v1/search?q=login&entity_type=bugs" \
  -H "Authorization: Bearer <token>"

# Search bugs with filters
curl "http://localhost:3000/api/v1/search/bugs?status=open&priority=high" \
  -H "Authorization: Bearer <token>"

# Search tasks with filters
curl "http://localhost:3000/api/v1/search/tasks?status=in_progress&assignee=alice" \
  -H "Authorization: Bearer <token>"
```

---

## Sync Backends

### Local Backend - Works (No-Op)

Default mode. Data stays local in `.agile/`. No network calls. Good for:
- Solo developers
- Offline work
- Testing

### HTTP Backend - Push Works

Configure in `.agile/config.yaml`:

```yaml
sync:
  enabled: true
  backend: http
  server_url: "http://localhost:3000"
  auto_sync: false
```

Then:
```bash
standup login                    # Authenticate
standup sync                     # Push/pull changes
standup status                   # Check sync state
```

**What works:** Pushing local changes to server.
**What's broken:** Version tracking always starts at 0, causing duplicate syncs.

### Git Backend - Broken

Push works (commits and pushes `.agile/` directory), but pull returns synthetic data instead of actual changes. **Don't rely on this yet.**

---

## What's Good for AI Agents

Every tool has `--json` flag for structured output:

```bash
$ bug list --json
[
  {
    "id": "BUG-1",
    "title": "Login fails on Safari",
    "status": "open",
    "priority": "high",
    "labels": ["browser", "auth"],
    "created_at": "2024-01-15T10:30:00Z"
  }
]
```

This makes it easy for AI agents to:
- Parse and understand current state
- Make decisions based on structured data
- Report back to humans in natural language

---

## What's NOT Working

| Feature | Status | Issue |
|---------|--------|-------|
| Git sync pull | Broken | Returns fake data |
| Incremental sync | Broken | Version always 0 |
| `task show` | Missing | Can't view individual tasks |
| `retro show` | Missing | Can't view individual items |
| Delete commands | Partial | Only `note delete` exists |
| Task board visual | Stub | Just prints counts |
| Conflict resolution | Not implemented | Struct exists, unused |
| Tests | None | Zero test coverage |

---

## Quick Start for Local Use

This is what works best today:

```bash
# Clone and build
git clone <repo>
cd agile-tools

# Install the tools you want
cargo install --path crates/standup
cargo install --path crates/bug
cargo install --path crates/task
cargo install --path crates/note

# Initialize in your project
cd /your/project
standup init
bug init
task init
note init

# Start using them
standup add -y "Set up agile tools"
task add "Review bug backlog"
bug new "Fix memory leak in worker" --priority high
note add "Sprint planning" --content "Focus on performance this sprint"

# Get structured output for AI agents
standup list --json
bug list --status open --json
task list --json
```

Data lives in `.agile/` directory. Commit it to your repo for basic version control.
