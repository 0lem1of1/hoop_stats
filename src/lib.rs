pub mod handlers;
pub mod ingest;
pub mod models;

use std::sync::Arc;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, post},
};
use dashmap::DashMap;
use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::broadcast;

use handlers::{auth, hot_stats, players};
use models::{PlayerStats, ServerMessage};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: Arc<str>,
    pub reqwest_client: Client,
    pub webhook_url: Arc<str>,
    pub app_base_url: Arc<str>,
    pub hot_stats: Arc<DashMap<i32, PlayerStats>>,
    pub tx: broadcast::Sender<ServerMessage>,
}

pub fn build_router(state: AppState) -> Router {
    // `.layer` only wraps routes registered before it, which is what keeps the
    // auth guard off /signup and /signin. Adding a protected route means adding
    // it above this line, not below.
    Router::<AppState>::new()
        .route("/players", get(players::get_all_players))
        .route("/players/{id}", get(players::get_player_by_id))
        .layer(from_fn_with_state(state.clone(), auth::auth_guard))
        .route("/signup", post(auth::signup))
        .route("/signin", post(auth::signin))
        .route("/forgot-password", post(auth::forgot_password))
        .route("/reset-password", post(auth::reset_password))
        .route("/ws/analytics", get(hot_stats::analytics_ws_handler))
        .with_state(state)
}
