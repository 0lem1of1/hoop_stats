use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

use crate::AppState;
use crate::models::{Player, PlayerStats, PlayerWithStats};

pub async fn get_all_players(
    State(state): State<AppState>,
) -> Result<Json<Vec<Player>>, StatusCode> {
    let players = sqlx::query_as::<_, Player>("SELECT * FROM players")
        .fetch_all(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(players))
}

pub async fn get_player_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<PlayerWithStats>, StatusCode> {
    let player = sqlx::query_as::<_, Player>("SELECT * FROM players WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
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
        .fetch_all(&state.pool)
        .await
        .map_err(|e| {
            eprintln!("Error fetching stats: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(PlayerWithStats { player, stats }))
}
