mod handlers;
mod models;

use axum::{routing::{get, post}, Router, middleware::from_fn_with_state};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{env, sync::Arc};
use tokio::{net::TcpListener, sync::broadcast};
use reqwest::Client;
use dashmap::DashMap;

use handlers::{auth, players, hot_stats};
use models::{PlayerStats, ServerMessage};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub reqwest_client: Client,
    pub webhook_url: String,
    pub app_base_url: String,
    pub hot_stats: Arc<DashMap<i32, PlayerStats>>,
    pub tx: broadcast::Sender<ServerMessage>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let webhook_url = env::var("WEBHOOK_URL").expect("WEBHOOK_URL must be set");
    let app_base_url = env::var("APP_BASE_URL").expect("APP_BASE_URL must be set");
    let reqwest_client = Client::new();

    // --- Hot Stats WebSocket infrastructure ---
    let hot_stats_cache = Arc::new(DashMap::<i32, PlayerStats>::new());
    let (tx, _rx) = broadcast::channel::<ServerMessage>(128);

    // Hydrate the in-memory cache from the database
    hot_stats::hydrate_cache(&pool, &hot_stats_cache)
        .await
        .expect("Failed to hydrate hot_stats cache");

    let state = AppState {
        pool,
        jwt_secret,
        reqwest_client,
        webhook_url,
        app_base_url,
        hot_stats: hot_stats_cache,
        tx,
    };

    println!("Connected to database");

    let app = Router::<AppState>::new()
    .route("/players", get(players::get_all_players))
    .route("/players/{id}", get(players::get_player_by_id))
    .layer(from_fn_with_state(state.clone(), auth::auth_guard))
    .route("/signup", post(auth::signup))
    .route("/signin", post(auth::signin))
    .route("/forgot-password", post(auth::forgot_password))
    .route("/reset-password", post(auth::reset_password))
    .route("/ws/analytics", get(hot_stats::analytics_ws_handler))
    .with_state(state);

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = format!("0.0.0.0:{port}");

    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));
    println!("Server running on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}
