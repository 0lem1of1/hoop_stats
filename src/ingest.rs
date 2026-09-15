//! Pulls per-game stats from stats.nba.com and pushes them to connected clients.
//!
//! stats.nba.com is an undocumented endpoint: it rejects requests without
//! browser-ish headers, and most of the `leaguedash*` family times out from
//! non-residential IPs. `leagueleaders` is the one that answers reliably, so
//! everything here is derived from its box-score columns.

use std::{collections::HashMap, time::Duration};

use serde::Deserialize;
use sqlx::PgPool;

use crate::{models::PlayerStats, AppState};
use crate::handlers::hot_stats::hydrate_cache;
use crate::models::ServerMessage;

const LEAGUE_LEADERS_URL: &str = "https://stats.nba.com/stats/leagueleaders";
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Deserialize)]
struct LeagueLeadersResponse {
    #[serde(rename = "resultSet")]
    result_set: ResultSet,
}

#[derive(Debug, Deserialize)]
struct ResultSet {
    headers: Vec<String>,
    #[serde(rename = "rowSet")]
    row_set: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug)]
pub struct IngestedPlayer {
    pub nba_player_id: i32,
    pub name: String,
    pub team: String,
    pub games_played: i32,
    pub minutes_per_game: f32,
    pub points_per_game: f32,
    pub true_shooting_pct: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("request to stats.nba.com failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("stats.nba.com returned an unexpected payload: {0}")]
    Shape(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// True shooting percentage: points per shooting possession, where the 0.44
/// weight is the standard estimate of how many free throw attempts end a
/// possession. Returns None when a player has taken no shots at all.
fn true_shooting_pct(points: f32, fga: f32, fta: f32) -> Option<f32> {
    let shooting_possessions = 2.0 * (fga + 0.44 * fta);
    (shooting_possessions > 0.0).then(|| points / shooting_possessions)
}

fn parse_rows(body: LeagueLeadersResponse) -> Result<Vec<IngestedPlayer>, IngestError> {
    // Index by header name rather than position — the column order is not
    // part of any contract and has changed before.
    let idx: HashMap<&str, usize> = body
        .result_set
        .headers
        .iter()
        .enumerate()
        .map(|(i, h)| (h.as_str(), i))
        .collect();

    let col = |name: &str| -> Result<usize, IngestError> {
        idx.get(name)
            .copied()
            .ok_or_else(|| IngestError::Shape(format!("missing column {name}")))
    };

    let (c_id, c_name, c_team, c_gp, c_min, c_fga, c_fta, c_pts) = (
        col("PLAYER_ID")?, col("PLAYER")?, col("TEAM")?, col("GP")?,
        col("MIN")?, col("FGA")?, col("FTA")?, col("PTS")?,
    );

    let mut out = Vec::with_capacity(body.result_set.row_set.len());
    for row in body.result_set.row_set {
        let num = |i: usize| row.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let text = |i: usize| row.get(i).and_then(|v| v.as_str()).unwrap_or("").to_string();

        let (pts, fga, fta) = (num(c_pts), num(c_fga), num(c_fta));
        let Some(ts) = true_shooting_pct(pts, fga, fta) else {
            continue; // player has never attempted a shot; nothing to say about them
        };

        out.push(IngestedPlayer {
            nba_player_id: num(c_id) as i32,
            name: text(c_name),
            team: text(c_team),
            games_played: num(c_gp) as i32,
            minutes_per_game: num(c_min),
            points_per_game: pts,
            true_shooting_pct: ts,
        });
    }

    if out.is_empty() {
        return Err(IngestError::Shape("no usable rows in response".to_string()));
    }
    Ok(out)
}

pub async fn fetch_league_leaders(
    client: &reqwest::Client,
    season: &str,
) -> Result<Vec<IngestedPlayer>, IngestError> {
    let body = client
        .get(LEAGUE_LEADERS_URL)
        .header("User-Agent", BROWSER_UA)
        .header("Referer", "https://www.nba.com/")
        .query(&[
            ("LeagueID", "00"),
            ("PerMode", "PerGame"),
            ("Scope", "S"),
            ("Season", season),
            ("SeasonType", "Regular Season"),
            ("StatCategory", "PTS"),
        ])
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?
        .json::<LeagueLeadersResponse>()
        .await?;

    parse_rows(body)
}

/// Upserts players and their season stat line, returning how many rows changed.
pub async fn persist(
    pool: &PgPool,
    season: &str,
    players: &[IngestedPlayer],
) -> Result<u64, IngestError> {
    let mut tx = pool.begin().await?;
    let mut written = 0u64;

    for p in players {
        let player_id = sqlx::query_scalar::<_, i32>(
            "INSERT INTO players (name, position, archetype, nba_player_id, team)
             VALUES ($1, 'N/A', NULL, $2, $3)
             ON CONFLICT (nba_player_id) DO UPDATE
                SET name = EXCLUDED.name, team = EXCLUDED.team
             RETURNING id",
        )
        .bind(&p.name)
        .bind(p.nba_player_id)
        .bind(&p.team)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO stats (player_id, season, points_per_game, true_shooting_pct,
                                games_played, minutes_per_game, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             ON CONFLICT (player_id, season) DO UPDATE
                SET points_per_game   = EXCLUDED.points_per_game,
                    true_shooting_pct = EXCLUDED.true_shooting_pct,
                    games_played      = EXCLUDED.games_played,
                    minutes_per_game  = EXCLUDED.minutes_per_game,
                    updated_at        = NOW()",
        )
        .bind(player_id)
        .bind(season)
        .bind(p.points_per_game)
        .bind(p.true_shooting_pct)
        .bind(p.games_played)
        .bind(p.minutes_per_game)
        .execute(&mut *tx)
        .await?;

        written += 1;
    }

    tx.commit().await?;
    Ok(written)
}

/// One ingest pass: fetch, persist, refresh the cache, tell connected clients.
pub async fn run_once(state: &AppState, season: &str) -> Result<u64, IngestError> {
    let players = fetch_league_leaders(&state.reqwest_client, season).await?;
    let written = persist(&state.pool, season, &players).await?;

    hydrate_cache(&state.pool, &state.hot_stats).await?;

    let snapshot: Vec<PlayerStats> =
        state.hot_stats.iter().map(|e| e.value().clone()).collect();

    // Both sends fail only when nobody is listening, which is not an error.
    let _ = state.tx.send(ServerMessage::GlobalAlert {
        message: format!("{season} stats refreshed for {written} players"),
    });
    let _ = state.tx.send(ServerMessage::AnalyticsUpdate(snapshot));

    Ok(written)
}

/// Background refresh loop. Failures are logged and retried on the next tick —
/// stats.nba.com rate-limits, and a stale cache beats a dead server.
pub fn spawn(state: AppState, season: String, every: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(every);
        loop {
            ticker.tick().await;
            match run_once(&state, &season).await {
                Ok(n) => println!("ingest: refreshed {n} players for {season}"),
                Err(e) => eprintln!("ingest failed, will retry in {:?}: {e}", every),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_pct_counts_two_points_per_field_goal_attempt() {
        // No free throws, so this is just points per 2 shot attempts:
        // 20 / (2 * 10) = 1.0, i.e. a player scoring 2 points every attempt.
        assert_eq!(true_shooting_pct(20.0, 10.0, 0.0).unwrap(), 1.0);
    }

    #[test]
    fn ts_pct_weights_free_throws_at_point_four_four() {
        // 25 / (2 * (20 + 0.44 * 5)) = 25 / 44.4
        let ts = true_shooting_pct(25.0, 20.0, 5.0).unwrap();
        assert!((ts - 25.0 / 44.4).abs() < 1e-6, "got {ts}");
    }

    #[test]
    fn ts_pct_is_none_without_shots() {
        assert!(true_shooting_pct(0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn parse_rows_indexes_by_header_name_not_position() {
        // Same data, columns deliberately reordered.
        let json = serde_json::json!({
            "resultSet": {
                "headers": ["PTS", "PLAYER", "FTA", "TEAM", "FGA", "GP", "PLAYER_ID", "MIN"],
                "rowSet": [[30.1, "Shai Gilgeous-Alexander", 8.1, "OKC", 20.6, 75, 1628983, 34.0]]
            }
        });
        let parsed = parse_rows(serde_json::from_value(json).unwrap()).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].nba_player_id, 1628983);
        assert_eq!(parsed[0].name, "Shai Gilgeous-Alexander");
        assert_eq!(parsed[0].team, "OKC");
    }

    #[test]
    fn parse_rows_rejects_missing_columns() {
        let json = serde_json::json!({
            "resultSet": { "headers": ["PLAYER"], "rowSet": [["someone"]] }
        });
        let err = parse_rows(serde_json::from_value(json).unwrap()).unwrap_err();
        assert!(matches!(err, IngestError::Shape(_)));
    }
}
