//! MCP server exposing the hoop_stats database to an AI assistant over stdio.
//!
//! Every number an assistant can quote here is computed in SQL and returned as
//! structured data. The model is only ever asked to interpret a comparison and
//! explain a tradeoff — it never derives a stat, because a model doing
//! arithmetic on a box score is a model that will occasionally be wrong about
//! it, and a wrong number is worse than no tool at all.

use std::env;

use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Debug, Serialize, sqlx::FromRow)]
struct StatLine {
    name: String,
    team: Option<String>,
    season: String,
    points_per_game: f32,
    true_shooting_pct: f32,
    games_played: Option<i32>,
    minutes_per_game: Option<f32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TopPerformersArgs {
    /// Only include players averaging at least this many points per game.
    #[serde(default)]
    min_ppg: Option<f32>,
    /// How many players to return. Defaults to 10, capped at 100.
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct PlayerArgs {
    /// Full or partial player name, case-insensitive.
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct CompareArgs {
    /// First player's name.
    name_a: String,
    /// Second player's name.
    name_b: String,
}

#[derive(Clone)]
struct HoopStats {
    pool: PgPool,
    tool_router: ToolRouter<Self>,
}

fn to_result<T: Serialize>(value: &T) -> Result<CallToolResult, ErrorData> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(json)]))
}

fn db_error(e: sqlx::Error) -> ErrorData {
    ErrorData::internal_error(format!("database query failed: {e}"), None)
}

#[tool_router]
impl HoopStats {
    fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tool_router: Self::tool_router(),
        }
    }

    /// List the best scorers, most points per game first. Use this to answer
    /// "who should I pick up" or "who is producing right now".
    #[tool]
    async fn top_performers(
        &self,
        Parameters(args): Parameters<TopPerformersArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = args.limit.unwrap_or(10).clamp(1, 100) as i64;
        let min_ppg = args.min_ppg.unwrap_or(0.0);

        let rows = sqlx::query_as::<_, StatLine>(
            "SELECT p.name, p.team, s.season, s.points_per_game, s.true_shooting_pct,
                    s.games_played, s.minutes_per_game
             FROM stats s
             JOIN players p ON p.id = s.player_id
             WHERE s.points_per_game >= $1
             ORDER BY s.points_per_game DESC
             LIMIT $2",
        )
        .bind(min_ppg)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db_error)?;

        to_result(&rows)
    }

    /// Look up one player's current season line by name.
    #[tool]
    async fn get_player(
        &self,
        Parameters(args): Parameters<PlayerArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let rows = find_by_name(&self.pool, &args.name).await?;
        if rows.is_empty() {
            return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "No player matching {:?}. Names come from stats.nba.com and only \
                 cover players in the scoring leaders feed.",
                args.name
            ))]));
        }
        to_result(&rows)
    }

    /// Put two players side by side. Returns both stat lines and nothing else —
    /// deciding who to start is a judgement call for the caller, and it depends
    /// on matchups and roster needs this database does not know about.
    #[tool]
    async fn compare_players(
        &self,
        Parameters(args): Parameters<CompareArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        #[derive(Serialize)]
        struct Comparison {
            a: Vec<StatLine>,
            b: Vec<StatLine>,
            note: &'static str,
        }

        let comparison = Comparison {
            a: find_by_name(&self.pool, &args.name_a).await?,
            b: find_by_name(&self.pool, &args.name_b).await?,
            note: "usage_rate is null for ingested rows: official usage needs team \
                   possession totals, which the upstream feed does not expose.",
        };

        to_result(&comparison)
    }
}

async fn find_by_name(pool: &PgPool, name: &str) -> Result<Vec<StatLine>, ErrorData> {
    sqlx::query_as::<_, StatLine>(
        "SELECT p.name, p.team, s.season, s.points_per_game, s.true_shooting_pct,
                s.games_played, s.minutes_per_game
         FROM stats s
         JOIN players p ON p.id = s.player_id
         WHERE unaccent(p.name) ILIKE unaccent($1)
         ORDER BY s.season DESC
         LIMIT 10",
    )
    .bind(format!("%{name}%"))
    .fetch_all(pool)
    .await
    .map_err(db_error)
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for HoopStats {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Season stats for NBA players, ingested from stats.nba.com. \
                 All figures are computed in SQL; quote them as returned rather \
                 than recalculating them.",
            );
        info.server_info = Implementation::new("hoop-stats", env!("CARGO_PKG_VERSION"));
        info
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let database_url =
        env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // stdout is the MCP transport, so anything human-readable goes to stderr.
    eprintln!("hoop-stats MCP server ready on stdio");

    let service = HoopStats::new(pool).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
