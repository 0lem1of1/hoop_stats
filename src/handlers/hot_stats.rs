use dashmap::DashMap;
use sqlx::PgPool;
use crate::{models::{PlayerStats, ClientMessage, ServerMessage}, AppState};

use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};

pub async fn hydrate_cache(pool: &PgPool, cache: &DashMap<i32, PlayerStats>) -> Result<(), sqlx::Error> {
    println!("Hydrating cache...");

    let rows = sqlx::query_as::<_, PlayerStats>(
        "SELECT * FROM stats"
    )
    .fetch_all(pool)
    .await
    .expect("Failed to fetch player stats");

    for stat in rows {
        cache.insert(stat.player_id, stat);
    }

    println!("Cache hydrated with {} entries", cache.len());
    Ok(())
}

pub async fn analytics_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut broadcast_rx = state.tx.subscribe();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(cmd) => {
                                match cmd {
                                    ClientMessage::GetTopPerformers { min_ppg } => {
                                        let results: Vec<PlayerStats> = state.hot_stats
                                            .iter()
                                            .filter(|e| e.points_per_game >= min_ppg)
                                            .map(|e| e.value().clone())
                                            .collect();

                                        let res = ServerMessage::AnalyticsUpdate(results);
                                        let _ = sender.send(Message::Text(
                                            serde_json::to_string(&res).unwrap().into()
                                        )).await;
                                    },
                                    ClientMessage::UpdateHypothetical { player_id, usage_adjust } => {
                                        if let Some(stats) = state.hot_stats.get(&player_id) {
                                            let new_ppg = stats.points_per_game * (1.0 + usage_adjust);
                                            let res = ServerMessage::HypotheticalResult {
                                                player_id,
                                                new_expected_points: new_ppg,
                                            };
                                            let _ = sender.send(Message::Text(
                                                serde_json::to_string(&res).unwrap().into()
                                            )).await;
                                        }
                                    },
                                    ClientMessage::Ping => {
                                        let _ = sender.send(Message::Text(
                                            serde_json::to_string(&ServerMessage::Pong).unwrap().into()
                                        )).await;
                                    }
                                }
                            },
                            Err(e) => {
                                let err = ServerMessage::Error { reason: format!("Invalid JSON: {}", e) };
                                let _ = sender.send(Message::Text(
                                    serde_json::to_string(&err).unwrap().into()
                                )).await;
                            }
                        }
                    }
                    _ => break,
                }
            }
            Ok(global_msg) = broadcast_rx.recv() => {
                let json = serde_json::to_string(&global_msg).unwrap();
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}