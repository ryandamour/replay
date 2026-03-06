# > [!WARNING]

This is me, a human, telling you that this plugin can be dangerous if not configured directly.  All of your messages will be stored / saved in .replay within your respective working directory.  This means if you hand off things like keys, secrets, or any other sensitive information, your message will be stored in a repo as soon as you commit.  I am not responsible if you don't take this message seriously.  This app was also 100% vibed - buyer beward.

Back to your regular AI programming:

# Replay

A Claude Code hook plugin that preserves every user prompt across context compactions, so Claude never loses the thread.

## The Problem

When Claude Code compacts context, it loses the raw history of what you asked, where things failed, and what direction the project is heading. You end up re-explaining context, re-stating goals, and watching Claude retrace steps you already covered.

## What Replay Does

Replay runs as a pair of Claude Code hooks:

1. **On every prompt** — silently captures your message into a hive-partitioned, git-tracked `.replay/` directory inside your project.
2. **After compaction** — injects a structured summary of your conversation history back into Claude's context, including topic clusters, retry patterns, and cross-session threads.

The result: Claude picks up where it left off. It knows what you asked three hours ago, what failed, and what you're building toward — even after context resets.

## Install

One command:

```sh
git clone <repo-url> && cd replay
./install.sh
```

Or step by step:

```sh
# 1. Build and install the binary
cargo install --path .

# 2. Install hooks + /replay slash command into Claude Code
replay install
```

`replay install` does three things:
- Writes `~/.claude/commands/replay.md` (the `/replay` slash command)
- Adds the `UserPromptSubmit` hook for capture to `~/.claude/settings.json`
- Adds the `SessionStart` hook for post-compaction analysis to `~/.claude/settings.json`

It's idempotent — safe to run multiple times.

## Usage

### Inside Claude Code

Type `/replay` in any Claude Code session:

```
/replay              → last 30 days of conversation history
/replay auth         → everything mentioning "auth"
/replay --all        → all messages, no time limit
/replay --today      → just today
/replay --last 90d   → last 90 days
/replay --all auth   → all messages mentioning "auth", ever
```

### CLI

You can also run it directly from your terminal:

```sh
replay analyze                          # last 30 days (default)
replay analyze --topic auth             # filter by topic keyword
replay analyze --all                    # everything
replay analyze --all --topic migration  # everything about migrations
replay analyze --today                  # just today
replay analyze --last 24h               # last 24 hours
replay analyze --last 90d               # last 90 days
replay analyze --sessions 5             # last 5 sessions
replay init                             # manually create .replay/ (normally auto-created)
```

## How It Works

```
You type a prompt
  → UserPromptSubmit hook fires
    → replay capture writes it to .replay/messages/user=you/date=.../hour=.../...md
      → git commits the change

Context compacts
  → SessionStart hook fires (matched on "compact")
    → replay analyze reads stored messages
      → partition-prunes date directories outside the time range
      → clusters by topic, detects retries, links cross-session threads
        → outputs a structured summary into Claude's new context
```

### Hive-Partitioned Storage

```
.replay/
  .git/                          # self-contained git repo
  .gitignore                     # ignores .lock file
  messages/
    user=alice/
      date=2026-03-06/
        hour=14/
          b3ca0f12_32-05-441.md  # session fragment + MM-SS-ms
  analysis/
    latest.md
```

The directory structure mirrors Hive-style partitioning. Date-based filters skip entire `date=` directories during the walk — the same predicate pushdown that Trino/Databend use against partitioned tables, just on a local filesystem.

Each message file is a markdown document with YAML front-matter (session ID, user, timestamp) followed by the raw prompt text.

### What Claude Sees After Compaction

```markdown
# Replay: Conversation History

The following is a chronological record of user prompts from this project,
preserved across context compaction. Use this to maintain continuity.

**Summary**: 12 messages across 3 sessions

## Topics

- **email** (4 messages): email, validation, failing, test, regex
- **middleware** (2 messages): middleware, refresh, jwt, token, logic

## Retry Patterns Detected

**Session ...b3ca0f12** — 3 attempts at a similar prompt:
  - *First* [14:32:05]: Fix the failing test for email validation
  - *Then* [14:45:11]: The email validation test is still failing try another approach
  - *Last* [15:02:33]: Email validation test keeps failing can we try a regex instead

## Cross-Session Threads

**auth, middleware** — spans 2 sessions across multiple days:
  - Session ...b3ca0f12 (2026-03-04): Set up JWT authentication middleware
  - Session ...f1e2d3c4 (2026-03-06): Fix token refresh for expired JWTs

## 2026-03-06 (Friday)

### Session ...b3ca0f12 (5 messages)

- **[14:32:05]** Set up the database migration for the users table
- **[14:35:22]** Add email validation to the User model
- **[14:55:13]** Fix the failing test for empty email addresses
...
```

## Analysis Features

**Semantic clustering** groups prompts by topic using keyword overlap, so Claude sees "you were working on auth" and "you were working on database migrations" rather than a flat list.

**Failure tracking** detects when consecutive prompts in a session have high word overlap — a signal that something isn't working and the user is rephrasing. Claude sees the retry sequence and knows what already didn't work.

**Cross-session continuity** links sessions across different dates that share significant topic overlap. If you started auth work on Tuesday and pick it back up Thursday, Claude knows it's the same thread.

**Partition pruning** skips entire date directories outside the requested time range during the filesystem walk. No wasted reads.
