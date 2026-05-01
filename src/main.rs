use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use std::env;
use tokio::net::TcpListener;

#[derive(Serialize, FromRow)]
struct Player {
    id: i32,
    name: String,
    position: String,
    archetype: String,
}

#[derive(Serialize, FromRow)]
struct PlayerStats {
    id: i32,
    player_id: i32,
    season: String,
    points_per_game: f32,
    true_shooting_pct: f32,
    usage_rate: f32,
}

#[derive(Serialize)]
struct PlayerWithStats {
    player: Player,
    stats: Vec<PlayerStats>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    println!("Connected to database");

    let app = Router::new()
        .route("/players", get(get_all_players))
        .route("/players/{id}", get(get_player_by_id))
        .with_state(pool);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn get_all_players(State(pool): State<PgPool>) -> Result<Json<Vec<Player>>, StatusCode> {
    let players = sqlx::query_as::<_, Player>("SELECT * FROM players")
        .fetch_all(&pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(players))
}


async fn get_player_by_id(
    State(pool): State<PgPool>,
    Path(id): Path<i32>,
) -> Result<Json<PlayerWithStats>, StatusCode> {
    let player = sqlx::query_as::<_, Player>("SELECT * FROM players WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("Error fetching player: {e}"); 
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let player = match player {
        Some(p) => p,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let stats = sqlx::query_as::<_, PlayerStats>("SELECT * FROM stats WHERE player_id = $1")
        .bind(id)
        .fetch_all(&pool)
        .await
        .map_err(|e| {
            eprintln!("Error fetching stats: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(PlayerWithStats { player, stats }))
}

