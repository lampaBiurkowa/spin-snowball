//! Talking to the Dibrysoft launcher running on this machine.
//!
//! The launcher puts an achievement endpoint and a session key in the
//! environment of the game it starts, and unlocking is one HTTP POST to
//! localhost. Both variables are missing when somebody runs the executable
//! directly, which is not an error: the game plays exactly the same, it just
//! cannot unlock anything.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

const ACHIEVEMENT_URL_VAR: &str = "DIBRYSOFT_ACHIEVEMENT_URL";
// the same key under two names, the second one predating achievements
const KEY_VARS: [&str; 2] = ["DIBRYSOFT_GAME_KEY", "DIBRYSOFT_AWARD_KEY"];

const TIMEOUT: Duration = Duration::from_secs(5);

struct Endpoint {
    addr: SocketAddr,
    host: String,
    path: String,
    key: String,
}

pub struct Launcher {
    // dropped along with the game, which ends the worker thread
    unlocks: Option<Sender<&'static str>>,
    // posting the same key twice is safe, so this only keeps the game from
    // sending one every frame the condition holds
    posted: HashSet<&'static str>,
}

impl Launcher {
    /// Reads what the launcher left in the environment, and starts the thread
    /// that does the posting. Never fails: without the variables, or with an
    /// endpoint that cannot be understood, unlocking is simply off.
    pub fn detect() -> Self {
        let url = std::env::var(ACHIEVEMENT_URL_VAR).ok().filter(|x| !x.is_empty());
        let key = KEY_VARS
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|value| !value.is_empty());

        let (Some(url), Some(key)) = (url, key) else {
            println!("Dibrysoft: no launcher session, achievements are off");
            return Self::off();
        };

        let Some(endpoint) = parse_endpoint(&url, key) else {
            eprintln!("Dibrysoft: {ACHIEVEMENT_URL_VAR} is not an address we can post to: {url}");
            return Self::off();
        };

        println!("Dibrysoft: unlocking achievements at {url}");

        let (tx, rx) = channel();
        thread::spawn(move || serve(endpoint, rx));

        Self {
            unlocks: Some(tx),
            posted: HashSet::new(),
        }
    }

    fn off() -> Self {
        Self {
            unlocks: None,
            posted: HashSet::new(),
        }
    }

    /// Hands one achievement to the launcher. The key has to be one the product
    /// declared in `.ndib/achievements.json`, or the launcher answers 404.
    pub fn unlock(&mut self, key: &'static str) {
        if !self.posted.insert(key) {
            return;
        }

        if let Some(unlocks) = &self.unlocks {
            let _ = unlocks.send(key);
        }
    }
}

fn parse_endpoint(url: &str, key: String) -> Option<Endpoint> {
    let url = url::Url::parse(url).ok()?;
    let host = url.host_str()?.to_string();
    let port = url.port_or_known_default()?;
    let addr = (host.as_str(), port).to_socket_addrs().ok()?.next()?;

    Some(Endpoint {
        addr,
        host: format!("{host}:{port}"),
        path: url.path().to_string(),
        key,
    })
}

/// Posts one unlock at a time, off the game thread, until the game goes away.
fn serve(endpoint: Endpoint, unlocks: Receiver<&'static str>) {
    for key in unlocks {
        match post(&endpoint, key) {
            Ok(response) => println!("Dibrysoft: {key} -> {response}"),
            // the launcher being unreachable is the player's problem to see in
            // the launcher, not the game's to react to
            Err(e) => eprintln!("Dibrysoft: could not unlock {key}: {e}"),
        }
    }
}

fn post(endpoint: &Endpoint, key: &str) -> std::io::Result<String> {
    let body = serde_json::json!({ "key": key }).to_string();

    // deliberately no Origin header: the endpoint refuses anything that looks
    // like it came from a browser
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         X-Dibrysoft-Award-Key: {key_header}\r\n\
         Content-Length: {length}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        path = endpoint.path,
        host = endpoint.host,
        key_header = endpoint.key,
        length = body.len(),
    );

    let mut stream = TcpStream::connect_timeout(&endpoint.addr, TIMEOUT)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    Ok(summarise(&response))
}

/// The status line and the body, which is all that is worth a log line.
fn summarise(response: &str) -> String {
    let status = response.lines().next().unwrap_or("no response").trim();
    match response.split_once("\r\n\r\n") {
        Some((_, body)) if !body.trim().is_empty() => format!("{status} {}", body.trim()),
        _ => status.to_string(),
    }
}

/// The counters the launcher watches.
///
/// Unlocks live in the launcher, but progress towards one does not, so anything
/// that adds up over several sessions is kept here. `.ndib/achievements.json`
/// points its `watch` rules at this file and the launcher unlocks those itself
/// while the game runs.
pub struct Stats {
    path: PathBuf,
    snowballs: u64,
    wins: u64,
    // one bit per game mode, so playing the same one twice still counts once
    modes: u32,
    dirty: bool,
    written: Instant,
}

impl Stats {
    /// Picks up where the last session left off. A file that is missing or
    /// unreadable just starts everything at zero.
    pub fn load() -> Self {
        // next to the game, the way its map is: the launcher starts it in its
        // own install directory, which is what {INSTALL} resolves to
        let path = PathBuf::from("stats.xml");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();

        Self {
            snowballs: number(&existing, "snowballs"),
            wins: number(&existing, "wins"),
            modes: number(&existing, "modeMask") as u32,
            path,
            dirty: false,
            written: Instant::now(),
        }
    }

    pub fn snowball_thrown(&mut self) {
        self.snowballs += 1;
        self.dirty = true;
    }

    pub fn match_won(&mut self) {
        self.wins += 1;
        self.dirty = true;
    }

    pub fn mode_played(&mut self, mode: u32) {
        let bit = 1 << mode;
        if self.modes & bit == 0 {
            self.modes |= bit;
            self.dirty = true;
        }
    }

    /// Writes at most once a second, which is how often the launcher looks.
    pub fn maybe_flush(&mut self) {
        if self.dirty && self.written.elapsed() >= Duration::from_secs(1) {
            self.flush();
        }
    }

    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }

        self.dirty = false;
        self.written = Instant::now();

        let contents = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!-- Watched by the Dibrysoft launcher, see .ndib/achievements.json -->\n\
             <stats>\n\
             \x20 <snowballs>{}</snowballs>\n\
             \x20 <wins>{}</wins>\n\
             \x20 <modes>{}</modes>\n\
             \x20 <modeMask>{}</modeMask>\n\
             </stats>\n",
            self.snowballs,
            self.wins,
            self.modes.count_ones(),
            self.modes,
        );

        // written beside itself and moved into place, so the launcher never
        // reads half a file
        if let Err(e) = write_atomically(&self.path, &contents) {
            eprintln!("Dibrysoft: could not save {}: {e}", self.path.display());
        }
    }
}

fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let temporary = path.with_extension("xml.new");
    std::fs::write(&temporary, contents)?;
    std::fs::rename(&temporary, path)
}

/// Reads one `<tag>123</tag>` back out of the file we wrote.
fn number(xml: &str, tag: &str) -> u64 {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");

    xml.split_once(&open)
        .and_then(|(_, rest)| rest.split_once(&close))
        .and_then(|(value, _)| value.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_counters_it_wrote() {
        let xml = "<stats>\n  <snowballs>512</snowballs>\n  <wins>3</wins>\n  <modeMask>19</modeMask>\n</stats>";

        assert_eq!(number(xml, "snowballs"), 512);
        assert_eq!(number(xml, "wins"), 3);
        assert_eq!(number(xml, "modeMask"), 19);
        assert_eq!(number(xml, "missing"), 0);
    }

    #[test]
    fn counts_each_mode_once() {
        let mut stats = Stats::load();
        stats.modes = 0;

        stats.mode_played(0);
        stats.mode_played(0);
        stats.mode_played(5);

        assert_eq!(stats.modes.count_ones(), 2);
    }

    #[test]
    fn an_endpoint_keeps_the_path_and_port() {
        let endpoint = parse_endpoint("http://127.0.0.1:25220/achievement", "abc".into()).unwrap();

        assert_eq!(endpoint.host, "127.0.0.1:25220");
        assert_eq!(endpoint.path, "/achievement");
        assert_eq!(endpoint.addr.port(), 25220);
    }

    #[test]
    fn nonsense_is_not_an_endpoint() {
        assert!(parse_endpoint("not a url", "abc".into()).is_none());
    }

    /// The endpoint is strict about all of this: no Origin header, the key in
    /// its own header, and a JSON content type.
    #[test]
    fn a_request_looks_like_the_endpoint_expects() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();

        let served = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let read = stream.read(&mut request).unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"key\":\"wound-up\",\"alreadyUnlocked\":false}")
                .unwrap();
            String::from_utf8_lossy(&request[..read]).into_owned()
        });

        let endpoint =
            parse_endpoint(&format!("http://127.0.0.1:{port}/achievement"), "s3cret".into()).unwrap();
        let response = post(&endpoint, "wound-up").unwrap();
        let request = served.join().unwrap();

        assert!(request.starts_with("POST /achievement HTTP/1.1\r\n"), "{request}");
        assert!(request.contains(&format!("Host: 127.0.0.1:{port}\r\n")), "{request}");
        assert!(request.contains("Content-Type: application/json\r\n"), "{request}");
        assert!(request.contains("X-Dibrysoft-Award-Key: s3cret\r\n"), "{request}");
        assert!(request.contains("Content-Length: 18\r\n"), "{request}");
        assert!(!request.to_lowercase().contains("origin"), "{request}");
        assert!(request.ends_with("{\"key\":\"wound-up\"}"), "{request}");
        assert!(response.contains("200 OK"), "{response}");
        assert!(response.contains("alreadyUnlocked"), "{response}");
    }

    #[test]
    fn the_same_key_is_only_sent_once() {
        let (tx, rx) = channel();
        let mut launcher = Launcher {
            unlocks: Some(tx),
            posted: HashSet::new(),
        };

        launcher.unlock("first-throw");
        launcher.unlock("first-throw");
        launcher.unlock("wound-up");
        drop(launcher);

        assert_eq!(rx.iter().collect::<Vec<_>>(), ["first-throw", "wound-up"]);
    }

    #[test]
    fn nothing_is_sent_without_a_launcher() {
        let mut launcher = Launcher::off();
        launcher.unlock("first-throw");

        assert!(launcher.posted.contains("first-throw"));
        assert!(launcher.unlocks.is_none());
    }

    #[test]
    fn a_response_is_logged_as_its_status_and_body() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"key\":\"first-throw\"}";

        assert_eq!(
            summarise(response),
            "HTTP/1.1 200 OK {\"key\":\"first-throw\"}"
        );
    }
}
