use std::{env, sync::Arc, time::Duration};

use dashmap::DashMap;
use hoop_stats::{AppState, build_router, handlers::hot_stats, ingest, models::ServerMessage};
use reqwest::Client;
use sqlx::postgres::PgPoolOptions;
use tokio::{net::TcpListener, sync::broadcast};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    // A fresh deploy gets an empty database, so bring the schema up before serving.
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let jwt_secret: Arc<str> = env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set")
        .into();
    let webhook_url: Arc<str> = env::var("WEBHOOK_URL")
        .expect("WEBHOOK_URL must be set")
        .into();
    let app_base_url: Arc<str> = env::var("APP_BASE_URL")
        .expect("APP_BASE_URL must be set")
        .into();

    let hot_stats_cache = Arc::new(DashMap::new());
    let (tx, _rx) = broadcast::channel::<ServerMessage>(128);

    hot_stats::hydrate_cache(&pool, &hot_stats_cache)
        .await
        .expect("Failed to hydrate hot_stats cache");

    let state = AppState {
        pool,
        jwt_secret,
        reqwest_client: Client::new(),
        webhook_url,
        app_base_url,
        hot_stats: hot_stats_cache,
        tx,
    };

    println!("Connected to database");

    let season = env::var("NBA_SEASON").unwrap_or_else(|_| "2025-26".to_string());
    let ingest_every = Duration::from_secs(
        env::var("INGEST_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600),
    );
    ingest::spawn(state.clone(), season, ingest_every);

    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = format!("0.0.0.0:{port}");

    let listener = TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("Failed to bind {addr}: {e}"));
    println!("Server running on http://{addr}");
    axum::serve(listener, build_router(state)).await.unwrap();
}
