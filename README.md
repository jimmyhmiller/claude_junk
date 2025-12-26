# agile

CLI agile tools for small teams, designed for AI agent interaction.

## Install

```bash
cargo install --path .
```

## Usage

Initialize in your project:
```bash
agile init
```

### Commands

| Command | Description |
|---------|-------------|
| `standup` | Daily standups - yesterday, today, blockers |
| `bug` | Bug/issue tracking |
| `retro` | Retrospectives - good, bad, action items |
| `task` | Kanban task board |
| `decision` | Architectural decision records |
| `note` | Meeting notes and context |
| `review` | Code review requests |
| `kudos` | Team appreciation |

### Examples

```bash
# Standup
agile standup add -y "Fixed auth bug" -t "Working on API" -b "Need design review"
agile standup today

# Bugs
agile bug new "Login fails on Safari" --priority high
agile bug list --status open
agile bug close abc123

# Retro
agile retro good "Shipped on time"
agile retro bad "Too many meetings"
agile retro action "Reduce meeting frequency" --owner alice

# Tasks
agile task add "Implement caching"
agile task board
agile task start abc123
agile task done abc123

# Decisions
agile decision record "Use PostgreSQL" --decision "Need ACID guarantees" --context "Evaluated MySQL, PostgreSQL, MongoDB"

# Notes
agile note add "Sprint planning" --content "Discussed Q1 priorities" --tag planning

# Reviews
agile review request "Add user auth" --reviewer alice --reviewer bob
agile review approve abc123

# Kudos
agile kudos give alice "Great job on the refactor!"
agile kudos leaderboard
```

### JSON Output

All commands support `--json` for agent parsing:

```bash
agile standup list --json
agile bug list --json
```

## Storage

Data is stored in `.agile/` in YAML format, designed to be git-friendly.
