use std::collections::HashSet;

use crate::{
    AppState,
    models::{ClientMessage, PlayerStats, ServerMessage},
};
use dashmap::DashMap;
use sqlx::PgPool;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};

pub async fn hydrate_cache(
    pool: &PgPool,
    cache: &DashMap<i32, PlayerStats>,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query_as::<_, PlayerStats>("SELECT * FROM stats")
        .fetch_all(pool)
        .await?;

    let mut live: HashSet<i32> = HashSet::with_capacity(rows.len());
    for stat in rows {
        live.insert(stat.player_id);
        cache.insert(stat.player_id, stat);
    }

    // Insert-only leaves players who were deleted from the database being served
    // from cache forever. Drop them after the refresh rather than clearing first,
    // so readers never observe an empty cache.
    cache.retain(|player_id, _| live.contains(player_id));

    println!("Cache hydrated with {} entries", cache.len());
    Ok(())
}

/// Serializing a ServerMessage can only fail on non-finite floats, which the
/// handlers below reject up front. The fallback keeps a bad value from taking
/// the whole connection down.
fn encode(msg: &ServerMessage) -> String {
    serde_json::to_string(msg).unwrap_or_else(|_| {
        r#"{"event":"Error","data":{"reason":"failed to encode response"}}"#.to_string()
    })
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
                                        let _ = sender.send(Message::Text(encode(&res).into())).await;
                                    },
                                    ClientMessage::UpdateHypothetical { player_id, usage_adjust } => {
                                        let res = match state.hot_stats.get(&player_id) {
                                            _ if !usage_adjust.is_finite() => ServerMessage::Error {
                                                reason: "usage_adjust must be a finite number".to_string(),
                                            },
                                            Some(stats) => {
                                                let new_ppg = stats.points_per_game * (1.0 + usage_adjust);
                                                if new_ppg.is_finite() {
                                                    ServerMessage::HypotheticalResult {
                                                        player_id,
                                                        new_expected_points: new_ppg,
                                                    }
                                                } else {
                                                    ServerMessage::Error {
                                                        reason: "usage_adjust produces an out-of-range result".to_string(),
                                                    }
                                                }
                                            }
                                            None => ServerMessage::Error {
                                                reason: format!("no cached stats for player {player_id}"),
                                            },
                                        };
                                        let _ = sender.send(Message::Text(encode(&res).into())).await;
                                    },
                                    ClientMessage::Ping => {
                                        let _ = sender.send(Message::Text(
                                            encode(&ServerMessage::Pong).into()
                                        )).await;
                                    }
                                }
                            },
                            Err(e) => {
                                let err = ServerMessage::Error { reason: format!("Invalid JSON: {e}") };
                                let _ = sender.send(Message::Text(encode(&err).into())).await;
                            }
                        }
                    }
                    _ => break,
                }
            }
            Ok(global_msg) = broadcast_rx.recv() => {
                if sender.send(Message::Text(encode(&global_msg).into())).await.is_err() {
                    break;
                }
            }
        }
    }
}
