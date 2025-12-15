use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, sync::mpsc};
use tokio_tungstenite::accept_async;
use tungstenite::Message;
use uuid::Uuid;

use crate::{GameState, MatchPhase, PeerMap, PlayerStatus, Team};

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
    Command {
        cmd: Command,
    },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "cmd")]
pub enum Command {
    Start {
        score_limit: Option<u32>,
        time_limit_secs: Option<u32>,
    },
    Stop,
    Pause,
    Resume,
    LoadMap {
        data: String,
    },
    JoinAsPlayer {
        team: Team,
    },
    JoinAsSpectator,
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
        scores: std::collections::HashMap<Team, u32>,
        ball: Option<BallState>,
        phase: MatchPhase,
        time_elapsed: f32,
        paused: bool
    },
    Pong {
        ts: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BallState {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerState {
    pub id: String,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub rot_deg: f32,
    pub status: PlayerStatus,
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
        gs.add_spectator(client_id.clone());
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
                        if let MatchPhase::Playing {
                            score_limit: _,
                            time_limit_secs: _,
                        } = gs.phase
                        {
                            gs.apply_input(&client_id_clone, left, right, shoot);
                        }
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
                    Ok(ClientMessage::Command { cmd }) => {
                        let mut gs = game_state_clone.lock().unwrap();
                        match cmd {
                            Command::Start {
                                score_limit,
                                time_limit_secs,
                            } => {
                                // Only allow start from Lobby or Finished
                                match gs.phase {
                                    MatchPhase::Lobby | MatchPhase::Finished => {
                                        gs.start_match(score_limit, time_limit_secs);
                                    }
                                    MatchPhase::Playing { .. } => {
                                        // already playing; optionally send a message back
                                    }
                                }
                            }
                            Command::Pause => {
                                println!("got pause");
                                gs.pause_match();
                            }
                            Command::Resume => {
                                println!("got resume");
                                gs.resume_match();
                            }
                            Command::Stop => {
                                println!("got stop");
                                gs.stop_match();
                            }
                            Command::LoadMap { data } => {
                                println!("got load");
                                gs.load_map(&data);
                            }
                            Command::JoinAsPlayer { team } => {
                                println!("got join team");
                                if let Some(p) = gs.players.get_mut(&client_id_clone) {
                                    p.status = PlayerStatus::Playing(team);
                                }
                            }
                            Command::JoinAsSpectator => {
                                println!("got join spectator");
                                if let Some(p) = gs.players.get_mut(&client_id_clone) {
                                    p.status = PlayerStatus::Spectator;
                                }
                            }
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
