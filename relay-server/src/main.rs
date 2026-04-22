use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{header, Method},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info};

#[derive(Clone, Default)]
struct RelayState {
    rooms: Arc<RwLock<HashMap<String, Vec<mpsc::UnboundedSender<Message>>>>>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct Envelope {
    pairing_token: String,
    device_id: String,
    payload: serde_json::Value,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE]);

    let state = RelayState::default();
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("Failed to bind to {}, trying random available port. Error: {}", addr, e);
            let fallback_addr = SocketAddr::from(([0, 0, 0, 0], 0));
            tokio::net::TcpListener::bind(fallback_addr).await.expect("bind relay listener on random port")
        }
    };

    let actual_addr = listener.local_addr().expect("get local address");
    info!("relay listening on {actual_addr}");

    axum::serve(listener, app).await.expect("serve relay");
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<RelayState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, socket))
}

async fn handle_socket(state: RelayState, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let mut joined_room: Option<String> = None;

    let send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if sender.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(message)) = receiver.next().await {
        match message {
            Message::Text(raw) => match serde_json::from_str::<Envelope>(&raw) {
                Ok(envelope) => {
                    let room = envelope.pairing_token.clone();
                    joined_room = Some(room.clone());

                    let mut rooms = state.rooms.write().await;
                    let peers = rooms.entry(room).or_default();
                    if !peers.iter().any(|peer| peer.same_channel(&tx)) {
                        peers.push(tx.clone());
                    }

                    let outbound = Message::Text(raw);
                    for peer in peers.iter() {
                        if !peer.same_channel(&tx) {
                            let _ = peer.send(outbound.clone());
                        }
                    }
                }
                Err(err) => error!("invalid relay envelope: {err}"),
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    if let Some(room) = joined_room {
        let mut rooms = state.rooms.write().await;
        if let Some(peers) = rooms.get_mut(&room) {
            peers.retain(|peer| !peer.same_channel(&tx));
            if peers.is_empty() {
                rooms.remove(&room);
            }
        }
    }

    send_task.abort();
}
