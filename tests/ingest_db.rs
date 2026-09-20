//! Database-backed tests for the two bugs that actually bit during development:
//! repeat ingests duplicating players, and the cache serving players that had
//! been deleted.
//!
//! These need Postgres. They skip themselves when DATABASE_URL is unset so that
//! `cargo test` still passes on a machine without one. Rows are namespaced by a
//! unique season string and negative NBA ids (real ids are positive), so a test
//! run cannot disturb ingested data.

use std::sync::Arc;

use dashmap::DashMap;
use hoop_stats::{
    handlers::hot_stats::hydrate_cache,
    ingest::{IngestedPlayer, persist},
};
use sqlx::{PgPool, postgres::PgPoolOptions};

async fn pool() -> Option<PgPool> {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .ok()?;
    sqlx::migrate!().run(&pool).await.expect("migrations");
    Some(pool)
}

fn fixture(nba_player_id: i32, name: &str, ppg: f32) -> IngestedPlayer {
    IngestedPlayer {
        nba_player_id,
        name: name.to_string(),
        team: "TST".to_string(),
        games_played: 10,
        minutes_per_game: 30.0,
        points_per_game: ppg,
        true_shooting_pct: 0.6,
    }
}

async fn cleanup(pool: &PgPool, ids: &[i32]) {
    sqlx::query("DELETE FROM players WHERE nba_player_id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await
        .expect("cleanup");
}

#[tokio::test]
async fn repeated_ingest_updates_instead_of_duplicating() {
    let Some(pool) = pool().await else { return };
    let season = format!("test-dup-{}", std::process::id());
    let ids = [-9001, -9002];

    cleanup(&pool, &ids).await;

    let first = vec![
        fixture(-9001, "Dup One", 20.0),
        fixture(-9002, "Dup Two", 10.0),
    ];
    persist(&pool, &season, &first).await.expect("first ingest");

    // Same players, new numbers — what every refresh after the first looks like.
    let second = vec![
        fixture(-9001, "Dup One", 25.5),
        fixture(-9002, "Dup Two", 11.0),
    ];
    persist(&pool, &season, &second)
        .await
        .expect("second ingest");

    let players: i64 =
        sqlx::query_scalar("SELECT count(*) FROM players WHERE nba_player_id = ANY($1)")
            .bind(&ids[..])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(players, 2, "second ingest inserted duplicate players");

    let ppg: f32 = sqlx::query_scalar(
        "SELECT s.points_per_game FROM stats s
         JOIN players p ON p.id = s.player_id
         WHERE p.nba_player_id = -9001 AND s.season = $1",
    )
    .bind(&season)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ppg, 25.5, "stat row was not updated by the second ingest");

    cleanup(&pool, &ids).await;
}

#[tokio::test]
async fn hydrate_cache_drops_players_deleted_from_the_database() {
    let Some(pool) = pool().await else { return };
    let season = format!("test-evict-{}", std::process::id());
    let ids = [-9101, -9102];

    cleanup(&pool, &ids).await;

    let players = vec![
        fixture(-9101, "Stay Here", 20.0),
        fixture(-9102, "Go Away", 10.0),
    ];
    persist(&pool, &season, &players).await.expect("ingest");

    let doomed: i32 = sqlx::query_scalar("SELECT id FROM players WHERE nba_player_id = -9102")
        .fetch_one(&pool)
        .await
        .unwrap();

    let cache = Arc::new(DashMap::new());
    hydrate_cache(&pool, &cache).await.expect("first hydrate");
    assert!(
        cache.contains_key(&doomed),
        "player missing after first hydrate"
    );

    sqlx::query("DELETE FROM players WHERE nba_player_id = -9102")
        .execute(&pool)
        .await
        .unwrap();

    hydrate_cache(&pool, &cache).await.expect("second hydrate");
    assert!(
        !cache.contains_key(&doomed),
        "deleted player is still being served from cache"
    );

    cleanup(&pool, &ids).await;
}
