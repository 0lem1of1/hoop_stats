mod handlers;
mod models;

use axum::{routing::{get, post}, Router, middleware::from_fn_with_state};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::env;
use tokio::net::TcpListener;
use reqwest::Client;

use handlers::{auth, players};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
    pub reqwest_client: Client,
    pub webhook_url: String,
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
    let reqwest_client = Client::new();
    
    let state = AppState {
        pool,
        jwt_secret,
        reqwest_client,
        webhook_url,
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
    .route("/webhook-receiver", post(auth::webhook_receiver))
    .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

