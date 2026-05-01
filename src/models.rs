use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// --- PLAYER MODELS ---

#[derive(Serialize, FromRow)]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub position: String,
    pub archetype: String,
}

#[derive(Serialize, FromRow)]
pub struct PlayerStats {
    pub id: i32,
    pub player_id: i32,
    pub season: String,
    pub points_per_game: f32,
    pub true_shooting_pct: f32,
    pub usage_rate: f32,
}

#[derive(Serialize)]
pub struct PlayerWithStats {
    pub player: Player,
    pub stats: Vec<PlayerStats>,
}

// --- AUTH MODELS ---

#[derive(Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub password: String,
    pub favorite_player_id: Option<i32>,
}

#[derive(Serialize)]
pub struct SignupResponse {
    pub message: String,
    pub user_id: i32,
}

#[derive(Deserialize)]
pub struct SigninRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SigninResponse {
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i32,
    pub exp: usize,
}