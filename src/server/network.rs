use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::accept_async;
use tungstenite::Message;
use uuid::Uuid;

use crate::{GameState, PeerMap};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Input {
        left: bool,
        right: bool,
        shoot: bool,
    },
    Ping {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AssignId {
        id: String,
    },
    WorldState {
        players: Vec<PlayerState>,
        snowballs: Vec<SnowballState>,
    },
    Pong {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerState {
    pub id: String,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub rot_deg: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnowballState {
    pub id: u64,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub life: f32,
}

pub async fn handle_connection(
    stream: TcpStream,
    peers: PeerMap,
    game_state: Arc<Mutex<GameState>>,
) -> anyhow::Result<()> {
    let ws = accept_async(stream).await?;
    let (mut ws_sender, mut ws_receiver) = ws.split();

    let client_id = Uuid::new_v4().to_string();
    println!("New client {}", client_id);
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    peers.lock().unwrap().insert(client_id.clone(), tx.clone());
    {
        let mut gs = game_state.lock().unwrap();
        gs.add_player(client_id.clone());
    }

    let assign = ServerMessage::AssignId {
        id: client_id.clone(),
    };
    ws_sender
        .send(Message::Text(serde_json::to_string(&assign)?.into()))
        .await?;

    let forward_out = async {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    let peers_clone = peers.clone();
    let game_state_clone = game_state.clone();
    let client_id_clone = client_id.clone();
    let inbound = async {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(txt) = msg {
                match serde_json::from_str::<ClientMessage>(&txt) {
                    Ok(ClientMessage::Input { left, right, shoot }) => {
                        // update player's input snapshot in game state
                        let mut gs = game_state_clone.lock().unwrap();
                        gs.apply_input(&client_id_clone, left, right, shoot);
                    }
                    Ok(ClientMessage::Ping { ts }) => {
                        // reply Pong
                        if let Some(tx) = peers_clone.lock().unwrap().get(&client_id_clone) {
                            let _ = tx.send(Message::Text(
                                serde_json::to_string(&ServerMessage::Pong { ts })
                                    .unwrap()
                                    .into(),
                            ));
                        }
                    }
                    Err(e) => {
                        println!("Malformed client msg: {e}");
                    }
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    tokio::select! {
        res = forward_out => { let _ = res; },
        res = inbound => { let _ = res; },
    }

    println!("Client {} disconnected", client_id);
    peers.lock().unwrap().remove(&client_id);
    {
        let mut gs = game_state.lock().unwrap();
        gs.remove_player(&client_id);
    }

    Ok(())
}
