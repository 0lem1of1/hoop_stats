use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// --- PLAYER MODELS ---

#[derive(Serialize, FromRow)]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub position: String,
    pub archetype: Option<String>,
}

#[derive(Debug, Serialize, FromRow, Clone)]
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
    pub purpose: String,
}

// --- PASSWORD RESET MODELS ---

#[derive(Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

// --- SENDGRID EMAIL MODELS ---

#[derive(Serialize)]
pub struct SendGridPayload {
    pub personalizations: Vec<Personalization>,
    pub from: EmailAddress,
    pub subject: String,
    pub content: Vec<Content>,
}

#[derive(Serialize)]
pub struct Personalization {
    pub to: Vec<EmailAddress>,
    pub subject: String,
}

#[derive(Serialize)]
pub struct EmailAddress {
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub content_type: String,
    pub value: String,
}

// --- LIVE UPDATE MODELS --- 

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", content = "data")]
pub enum ClientMessage {
    GetTopPerformers { min_ppg: f32 },
    UpdateHypothetical { player_id: i32, usage_adjust: f32 },
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum ServerMessage {
    AnalyticsUpdate(Vec<PlayerStats>),
    HypotheticalResult { player_id: i32, new_expected_points: f32 },
    GlobalAlert { message: String },
    Pong,
    Error { reason: String },
}