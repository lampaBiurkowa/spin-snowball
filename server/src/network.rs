use std::{sync::{Arc, Mutex}, time::Duration};

use futures::{SinkExt, StreamExt};
use spin_snowball_shared::*;
use tokio::{net::TcpStream, sync::mpsc, time::sleep};
use tokio_tungstenite::accept_async;
use tungstenite::Message;
use uuid::Uuid;

use crate::{ClientOut, GameState, MatchPhase, PeerMap, PlayerStatus, Team};

pub async fn handle_connection(
    stream: TcpStream,
    peers: PeerMap,
    game_state: Arc<Mutex<GameState>>,
) {
    let ws = accept_async(stream).await.unwrap();
    let (mut ws_sender, mut ws_receiver) = ws.split();

    let client_id = Uuid::new_v4().to_string();
    println!("New client {}", client_id);
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();
    let latest_world = Arc::new(Mutex::new(None));

    peers.lock().unwrap().insert(
        client_id.clone(),
        ClientOut {
            tx: tx.clone(),
            latest_world: latest_world.clone(),
        },
    );

    let map = {
        let mut gs = game_state.lock().unwrap();
        gs.add_new_player(client_id.clone());
        gs.map.clone()
    };

    let assign = ServerMessage::AssignId {
        id: client_id.clone(),
    };
    ws_sender
        .send(Message::Text(
            serde_json::to_string(&assign).unwrap().into(),
        ))
        .await
        .unwrap();

    let map = ServerMessage::Map { map };
    ws_sender
        .send(Message::Text(serde_json::to_string(&map).unwrap().into()))
        .await
        .unwrap();
    println!("just sent map");

    let forward_out = async {
        loop {
            // ---- take world state WITHOUT holding lock across await ----
            let world_msg = {
                let mut guard = latest_world.lock().unwrap();
                guard.take()
            };

            if let Some(msg) = world_msg {
                let txt = serde_json::to_string(&msg).unwrap();
                if ws_sender.send(Message::Text(txt.into())).await.is_err() {
                    continue;
                }
            }

            // ---- reliable messages ----
            while let Ok(msg) = rx.try_recv() {
                let txt = serde_json::to_string(&msg).unwrap();
                if ws_sender.send(Message::Text(txt.into())).await.is_err() {
                    continue;
                }
            }
            sleep(Duration::from_millis(16)).await;
        }
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
                            let _ = tx.tx.send(ServerMessage::Pong { ts });
                        }
                    }
                    Ok(ClientMessage::Command { cmd }) => {
                        let mut gs = game_state_clone.lock().unwrap();
                        match cmd {
                            Command::Start {
                                score_limit,
                                time_limit_secs,
                            } => {
                                match gs.phase {
                                    MatchPhase::Lobby => {
                                        if gs.players.iter().any(|(_, player)| {
                                            player.status != PlayerStatus::Spectator
                                        }) {
                                            gs.start_match(score_limit, time_limit_secs);
                                        } else {
                                            println!(
                                                "Noone belongs to any team - cannot start a match"
                                            );
                                        }
                                    }
                                    MatchPhase::Playing { .. } => {
                                        // already playing; optionally send a message back
                                    }
                                }
                            }
                            Command::Pause => {
                                gs.pause_match();
                            }
                            Command::Resume => {
                                gs.resume_match();
                            }
                            Command::Stop => {
                                gs.stop_match();
                            }
                            Command::LoadMap { data } => {
                                gs.load_map(&data);
                                let peers_guard = peers.lock().unwrap();
                                for (_id, tx) in peers_guard.iter() {
                                    println!("SENDIN");
                                    let _ = tx.tx.send(ServerMessage::Map {
                                        map: gs.map.clone(),
                                    });
                                }
                            }
                            Command::JoinAsPlayer { team } => {
                                if let Some(p) = gs.players.get_mut(&client_id_clone) {
                                    println!("got join team");
                                    p.status = PlayerStatus::Playing(team);
                                }
                            }
                            Command::JoinAsSpectator => {
                                if let Some(p) = gs.players.get_mut(&client_id_clone) {
                                    p.status = PlayerStatus::Spectator;
                                }
                            }
                            Command::SetNick { nick } => {
                                if let Some(p) = gs.players.get_mut(&client_id_clone) {
                                    p.nick = nick;
                                }
                            }
                            Command::SetColorDef { color, team } => match team {
                                Team::Team1 => gs.team1_color = color,
                                Team::Team2 => gs.team2_color = color,
                            },
                            Command::SetPhysicsSettings { settings } => {
                                gs.map.physics = settings.clone();
                                let peers_guard = peers.lock().unwrap();
                                for (_id, tx) in peers_guard.iter() {
                                    let _ = tx.tx.send(ServerMessage::PhysicsSettings {
                                        settings: settings.clone(),
                                    });
                                }
                            }
                            Command::SetGameMode {
                                game_mode,
                                action_target_time,
                            } => {
                                gs.game_mode = game_mode;
                                gs.action_target_time = action_target_time;
                            }
                        }
                    }
                    Err(e) => {
                        println!("Malformed client msg: {e}");
                    }
                }
            }
        }
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
}
