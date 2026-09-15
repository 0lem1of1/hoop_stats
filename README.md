# hoop_stats

A stats service for a fantasy basketball league, and an MCP server that lets you
ask Claude about your roster using those stats.

Our league argues about lineups in a group chat. The argument is usually
"he's been scoring a ton lately" versus "yeah, on twenty-five shots a game."
Points per game is the stat everyone quotes and the one that settles the least.
So this pulls the real numbers nightly, computes true shooting percentage from
the box score, and exposes both over an API, a live WebSocket feed, and a set of
MCP tools — so the answer to "start Player A or Player B?" comes from the same
place every time, instead of whoever sounds most confident.

---

## Quick start

Requires Rust 1.90+, Docker (for Postgres), and nothing else — the repo ships a
committed `.sqlx` query cache, so **it compiles without a running database.**

```bash
git clone https://github.com/0lem1of1/hoop_stats.git
cd hoop_stats

docker compose up -d                 # Postgres on :5432
cp .env.example .env                 # then set JWT_SECRET (see below)

cargo run                            # migrates, ingests, serves on :3000
```

On boot the server runs its migrations, pulls the current season's scoring
leaders from stats.nba.com, and starts serving. First ingest takes a second or
two; you should see:

```
Cache hydrated with 234 entries
Server running on http://0.0.0.0:3000
ingest: refreshed 234 players for 2025-26
```

Generate a real secret rather than using the placeholder:

```bash
echo "JWT_SECRET=$(openssl rand -hex 32)" >> .env
```

### Try it

```bash
# Create an account and get a token
curl -X POST localhost:3000/signup -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"correcthorsebattery"}'

TOKEN=$(curl -s -X POST localhost:3000/signin -H 'Content-Type: application/json' \
  -d '{"email":"you@example.com","password":"correcthorsebattery"}' | jq -r .token)

curl -s localhost:3000/players -H "Authorization: Bearer $TOKEN" | jq '.[0]'
```

---

## The MCP server

`src/bin/mcp.rs` is a Model Context Protocol server over stdio. Point Claude
Code (or any MCP client) at it and ask roster questions in plain language:

```bash
claude mcp add hoop-stats -- cargo run --quiet --bin mcp
```

Or wire it up manually:

```json
{
  "mcpServers": {
    "hoop-stats": {
      "command": "/path/to/hoop_stats/target/release/mcp",
      "env": { "DATABASE_URL": "postgres://postgres:postgres@localhost:5432/hoop_stats" }
    }
  }
}
```

Three tools: `top_performers`, `get_player`, `compare_players`.

**Every number these return is computed in SQL.** The model is asked to
interpret a comparison and explain a tradeoff — never to derive a statistic. A
model doing arithmetic on a box score will occasionally be wrong about it, and a
confidently wrong number is worse than no tool at all. `compare_players`
deliberately returns two stat lines and no recommendation: who to start depends
on matchups and roster needs this database doesn't know about.

Name lookups fold accents, so `doncic` finds `Dončić`.

---

## API

| Method | Path | Auth | Purpose |
|---|---|:-:|---|
| POST | `/signup` | — | Create an account. `409` if taken, `400` on a short password or unknown `favorite_player_id` |
| POST | `/signin` | — | Returns a JWT, valid 24h |
| POST | `/forgot-password` | — | Emails a reset link. Always `200`, so it can't be used to probe for registered addresses |
| POST | `/reset-password` | — | Consumes the emailed token |
| GET | `/players` | Bearer | Every player |
| GET | `/players/{id}` | Bearer | One player with their stat lines |
| GET | `/ws/analytics` | — | WebSocket, see below |

### WebSocket protocol

Client messages are `{"cmd": ..., "data": ...}`:

```json
{"cmd": "GetTopPerformers", "data": {"min_ppg": 25.0}}
{"cmd": "UpdateHypothetical", "data": {"player_id": 4, "usage_adjust": 0.1}}
{"cmd": "Ping"}
```

Server messages are `{"event": ..., "data": ...}` — `AnalyticsUpdate`,
`HypotheticalResult`, `GlobalAlert`, `Pong`, `Error`.

`AnalyticsUpdate` and `GlobalAlert` also arrive **unprompted** whenever an
ingest completes, which is the point of the socket: connected clients see new
numbers without polling.

---

## Configuration

| Variable | Required | Default | Notes |
|---|:-:|---|---|
| `DATABASE_URL` | yes | — | Postgres connection string |
| `JWT_SECRET` | yes | — | Signs auth tokens. Generate your own |
| `APP_BASE_URL` | yes | — | Public URL, used to build reset links |
| `WEBHOOK_URL` | yes | — | SendGrid send-mail endpoint |
| `PORT` | no | `3000` | |
| `NBA_SEASON` | no | `2025-26` | |
| `INGEST_INTERVAL_SECS` | no | `3600` | stats.nba.com rate-limits; hourly is plenty |
| `SENDGRID_API_KEY` | password reset only | — | |
| `SENDGRID_FROM_EMAIL` | password reset only | — | Must be a verified sender |

Everything except the SendGrid pair is needed to boot. Password reset is the
only feature that degrades without them.

---

## Tests

```bash
cargo test
```

Unit tests cover the true-shooting formula and the upstream-response parser.
Two tests in `tests/ingest_db.rs` need Postgres and **skip themselves** when
`DATABASE_URL` is unset. Both are regressions for bugs this actually had: repeat
ingests duplicating players, and the in-memory cache serving players that had
been deleted from the database.

---

## Deploying

```bash
fly launch --no-deploy          # edit the app name in fly.toml first
fly postgres create && fly postgres attach <db>
fly secrets set JWT_SECRET=... APP_BASE_URL=https://<app>.fly.dev \
                WEBHOOK_URL=https://api.sendgrid.com/v3/mail/send
fly deploy
```

Migrations run at startup, so a fresh database needs no extra step. The
`Dockerfile` builds with `SQLX_OFFLINE=true` and `.dockerignore` keeps `.env`
out of the build context.

---

## How it works

```
stats.nba.com ──► ingest loop ──► Postgres ──► DashMap cache ──► WebSocket
                  (hourly)                        │                clients
                                                  └──► REST + MCP tools
```

- `src/ingest.rs` — fetch, compute, upsert, broadcast
- `src/handlers/hot_stats.rs` — WebSocket protocol and the cache
- `src/handlers/auth.rs` — argon2 hashing, JWT issue and verify
- `src/bin/mcp.rs` — the MCP server
- `src/lib.rs` — shared state and the router

Stats come from the `leagueleaders` endpoint. Points per game is taken as
given; true shooting is computed as `PTS / (2 × (FGA + 0.44 × FTA))`.

---

## Known limitations

- **`usage_rate` is null for ingested players.** Real usage rate needs team
  possession totals, and the stats.nba.com endpoint that serves them times out
  from non-residential IPs. Storing null beats storing a number that looks
  official and isn't.
- **Only scoring leaders.** The feed covers roughly 230 players, so deep-bench
  pickups won't be there.
- **`position` is `N/A`** for ingested players — the feed doesn't include it.
- **stats.nba.com is undocumented** and rejects requests without browser-like
  headers. A failed ingest is logged and retried on the next tick; the cache
  serves the previous numbers meanwhile.
- **JWTs can't be revoked.** No logout or blacklist; a leaked token is good for
  24 hours.
- **No rate limiting** on any endpoint.
- `/players` requires a token even though it's public data. That's inherited
  from when this was only ever going to be a private league tool.
