mod handlers;
mod models;

use axum::{routing::{get, post}, Router, middleware::from_fn_with_state};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::env;
use tokio::net::TcpListener;

use handlers::{auth, players};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub jwt_secret: String,
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
    
    let state = AppState {
        pool,
        jwt_secret,
    };

    println!("Connected to database");

    let app = Router::<AppState>::new()
    .route("/players", get(players::get_all_players))
    .route("/players/{id}", get(players::get_player_by_id))
    .layer(from_fn_with_state(state.clone(), auth::auth_guard))
    .route("/signup", post(auth::signup))
    .route("/signin", post(auth::signin))
    .with_state(state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

