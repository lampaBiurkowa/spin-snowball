use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::Duration;
use tungstenite::{Message, connect};
use url::Url;

#[derive(Serialize, Deserialize, Debug, Clone)]
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
        cmd: ClientCommand,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "cmd")]
pub enum ClientCommand {
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
    SetNick {
        nick: String,
    },
    SetTeamColor {
        color: TeamColor,
        team: Team,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Team {
    Team1,
    Team2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MatchPhase {
    Lobby,
    Playing {
        score_limit: Option<u32>,
        time_limit_secs: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TeamColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    AssignId {
        id: String,
    },
    WorldState {
        players: Vec<PlayerState>,
        snowballs: Vec<SnowballState>,
        scores: std::collections::HashMap<String, u32>,
        ball: Option<BallState>,
        phase: MatchPhase,
        time_elapsed: f32,
        paused: bool,
        team1_color: TeamColor,
        team2_color: TeamColor,
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
    pub nick: String,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub rot_deg: f32,

    pub status: PlayerStatus,
    pub team: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PlayerStatus {
    Spectator,
    Playing(Team),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SnowballState {
    pub id: u64,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub life: f32,
}

pub fn spawn_network_thread(server_addr: &str) -> (Sender<ClientMessage>, Receiver<ServerMessage>) {
    let (to_net_tx, to_net_rx) = channel::<ClientMessage>();
    let (from_net_tx, from_net_rx) = channel::<ServerMessage>();
    let server = server_addr.to_string();

    thread::spawn(move || {
        let url = Url::parse(&format!("ws://{}", server)).expect("Invalid WebSocket URL");
        println!("Connecting to {}", url);

        let (mut socket, _response) = match connect(url.to_string()) {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("WebSocket connect error: {}", e);
                return;
            }
        };

        // Optional: set read timeout so thread doesn’t block forever
        // if let Some(underlying) = socket.get_mut().get_mut() {
        //     let _ = underlying.set_read_timeout(Some(Duration::from_millis(10)));
        // }

        loop {
            // 1. Send all pending outbound messages
            while let Ok(msg) = to_net_rx.try_recv() {
                if let Ok(txt) = serde_json::to_string(&msg) {
                    if socket.send(Message::Text(txt.into())).is_err() {
                        eprintln!("Write error, closing network thread");
                        return;
                    }
                }
            }

            // 2. Try to read one incoming message (blocking up to 10 ms)
            match socket.read() {
                Ok(Message::Text(txt)) => {
                    if let Ok(sm) = serde_json::from_str::<ServerMessage>(&txt) {
                        let _ = from_net_tx.send(sm);
                    }
                }
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // just timeout, no problem
                }
                Err(tungstenite::Error::ConnectionClosed) => {
                    println!("Server closed connection");
                    return;
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    return;
                }
                _ => {}
            }

            // Small sleep to avoid busy loop
            thread::sleep(Duration::from_millis(2));
        }
    });

    (to_net_tx, from_net_rx)
}

pub struct NetworkClient {
    tx: Sender<ClientMessage>,
    rx: Receiver<ServerMessage>,
}

impl NetworkClient {
    pub fn new(server_addr: &str) -> Self {
        let (tx, rx) = spawn_network_thread(server_addr);
        Self { tx, rx }
    }

    pub fn send(&self, msg: ClientMessage) {
        let _ = self.tx.send(msg);
    }

    pub fn poll(&self) -> Option<ServerMessage> {
        match self.rx.try_recv() {
            Ok(msg) => Some(msg),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                println!("Network thread disconnected");
                None
            }
        }
    }
}
