//! Background music (Jellyfin albums): one mpg123 in remote mode (-R) plays
//! the queue through PulseAudio, so songs go on while Luma shows other pages
//! or another app is in front. Luma only sends commands and reads its state.
use crate::player::{Music, MusicSong};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Position is asked for this often while a song plays.
const POLL: Duration = Duration::from_millis(500);
/// Previous within this many seconds of a song goes to the song before it;
/// later it starts the song again.
const RESTART_AFTER: f32 = 3.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Loading,
    Playing,
    Paused,
}

#[derive(Clone, Debug)]
pub struct NowPlaying {
    /// Card id of the album.
    pub album_id: i64,
    pub album: String,
    pub artist: String,
    /// Downloaded cover, when the album card had one.
    pub cover: Option<String>,
    pub songs: Vec<MusicSong>,
    /// Play order: song indexes (shuffled or in album order).
    pub order: Vec<usize>,
    /// Place in `order`.
    pub at: usize,
    pub shuffle: bool,
    pub repeat: bool,
    pub status: Status,
    /// Seconds into the song, and its length.
    pub position: f32,
    pub duration: f32,
    /// Last error mpg123 reported (a song the server could not stream).
    pub error: Option<String>,
}

impl NowPlaying {
    /// Index in `songs` of the song playing.
    pub fn song(&self) -> usize {
        self.order.get(self.at).copied().unwrap_or(0)
    }

    pub fn current(&self) -> Option<&MusicSong> {
        self.songs.get(self.song())
    }

    pub fn progress(&self) -> f32 {
        if self.duration > 0. {
            (self.position / self.duration).clamp(0., 1.)
        } else {
            0.
        }
    }
}

#[derive(Debug, PartialEq)]
enum Event {
    /// @P: 0 stopped (or the song ended), 1 paused, 2 playing.
    State(u8),
    /// @S: the song's sample rate.
    Rate(u32),
    /// @SAMPLE: samples played and total.
    Sample(u64, u64),
    Error(String),
}

fn parse(line: &str) -> Option<Event> {
    let mut words = line.split_whitespace();
    match words.next()? {
        "@P" => words.next()?.parse().ok().map(Event::State),
        "@S" => words.nth(2)?.parse().ok().map(Event::Rate),
        "@SAMPLE" => {
            let at = words.next()?.parse().ok()?;
            let total = words.next()?.parse().ok()?;
            Some(Event::Sample(at, total))
        }
        "@E" => Some(Event::Error(
            line.trim_start_matches("@E").trim().to_owned(),
        )),
        _ => None,
    }
}

/// A shuffled order that starts with `first`.
fn shuffled(len: usize, first: usize) -> Vec<usize> {
    let mut rest: Vec<usize> = (0..len).filter(|i| *i != first).collect();
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
        | 1;
    for i in (1..rest.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        rest.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    let mut order = vec![first];
    order.extend(rest);
    order
}

#[derive(Default)]
pub struct Engine {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    events: Option<mpsc::Receiver<Event>>,
    pub now: Option<NowPlaying>,
    rate: u32,
    polled: Option<Instant>,
    /// A LOAD was sent: its old song's stop is not the end of the queue.
    loading: bool,
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.quit();
    }
}

fn pid_file() -> std::path::PathBuf {
    std::env::temp_dir().join("luma-music.pid")
}

impl Engine {
    /// Start an album at `start` (in album order); the engine starts mpg123
    /// the first time.
    pub fn start(&mut self, album_id: i64, music: Music, cover: Option<String>) {
        if music.songs.is_empty() {
            return;
        }
        let start = music.start.min(music.songs.len() - 1);
        let order = if music.shuffle {
            shuffled(music.songs.len(), start)
        } else {
            (0..music.songs.len()).collect()
        };
        let at = if music.shuffle { 0 } else { start };
        let repeat = self.now.as_ref().is_some_and(|n| n.repeat);
        self.now = Some(NowPlaying {
            album_id,
            album: music.album,
            artist: music.artist,
            cover,
            songs: music.songs,
            order,
            at,
            shuffle: music.shuffle,
            repeat,
            status: Status::Loading,
            position: 0.,
            duration: 0.,
            error: None,
        });
        self.load();
    }

    fn spawn(&mut self) -> bool {
        if self.child.is_some() {
            return true;
        }
        // An mpg123 left by an earlier Luma that did not close cleanly.
        if let Ok(pid) = std::fs::read_to_string(pid_file())
            && let Ok(pid) = pid.trim().parse::<i32>()
        {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
        let output = if cfg!(target_os = "macos") {
            "coreaudio"
        } else {
            "pulse"
        };
        let child = Command::new("mpg123")
            .args(["-R", "-o", output])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            if let Some(now) = self.now.as_mut() {
                now.error = Some("No music player on this TV".into());
            }
            return false;
        };
        let _ = std::fs::write(pid_file(), child.id().to_string());
        let (tx, rx) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if let Some(event) = parse(&line)
                        && tx.send(event).is_err()
                    {
                        break;
                    }
                }
            });
        }
        self.stdin = child.stdin.take();
        self.child = Some(child);
        self.events = Some(rx);
        // Only the answers Luma asks for: no line per decoded frame.
        self.send("SILENCE");
        true
    }

    fn send(&mut self, command: &str) {
        let failed = self.stdin.as_mut().is_none_or(|stdin| {
            writeln!(stdin, "{command}")
                .and_then(|()| stdin.flush())
                .is_err()
        });
        if failed {
            // mpg123 went away: the next command starts a new one.
            self.child = None;
            self.stdin = None;
            self.events = None;
        }
    }

    fn load(&mut self) {
        let Some(url) = self
            .now
            .as_ref()
            .and_then(|n| n.current())
            .map(|s| s.url.clone())
        else {
            return;
        };
        if !self.spawn() {
            return;
        }
        if let Some(now) = self.now.as_mut() {
            now.status = Status::Loading;
            now.position = 0.;
            now.duration = now.current().map_or(0., |s| s.ms as f32 / 1000.);
            now.error = None;
        }
        self.loading = true;
        self.send(&format!("LOAD {url}"));
    }

    pub fn toggle(&mut self) {
        let Some(status) = self.now.as_ref().map(|n| n.status) else {
            return;
        };
        if self.child.is_none() {
            self.load();
            return;
        }
        self.send("PAUSE");
        if let Some(now) = self.now.as_mut() {
            now.status = match status {
                Status::Paused => Status::Playing,
                _ => Status::Paused,
            };
        }
    }

    pub fn next(&mut self) {
        let Some(now) = self.now.as_mut() else { return };
        if now.at + 1 < now.order.len() {
            now.at += 1;
        } else if now.repeat {
            now.at = 0;
        } else {
            return;
        }
        self.load();
    }

    pub fn prev(&mut self) {
        let Some(now) = self.now.as_mut() else { return };
        if now.position <= RESTART_AFTER {
            if now.at > 0 {
                now.at -= 1;
            } else if now.repeat {
                now.at = now.order.len() - 1;
            }
        }
        self.load();
    }

    /// Play song `index` (album order) of the album playing.
    pub fn play_song(&mut self, index: usize) {
        let Some(now) = self.now.as_mut() else { return };
        if let Some(at) = now.order.iter().position(|i| *i == index) {
            now.at = at;
            self.load();
        }
    }

    pub fn toggle_shuffle(&mut self) {
        let Some(now) = self.now.as_mut() else { return };
        let song = now.song();
        now.shuffle = !now.shuffle;
        if now.shuffle {
            now.order = shuffled(now.songs.len(), song);
            now.at = 0;
        } else {
            now.order = (0..now.songs.len()).collect();
            now.at = song;
        }
    }

    pub fn toggle_repeat(&mut self) {
        if let Some(now) = self.now.as_mut() {
            now.repeat = !now.repeat;
        }
    }

    fn quit(&mut self) {
        if self.child.is_some() {
            self.send("QUIT");
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(pid_file());
        }
        self.stdin = None;
        self.events = None;
    }

    pub fn playing(&self) -> bool {
        self.now
            .as_ref()
            .is_some_and(|n| n.status == Status::Playing)
    }

    /// Apply what mpg123 said and ask for the position; true when the
    /// mini-player has something new to show.
    pub fn tick(&mut self) -> bool {
        let events: Vec<Event> = self
            .events
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        let mut changed = !events.is_empty();
        let mut ended = false;
        for event in events {
            match event {
                Event::Rate(rate) => self.rate = rate,
                Event::Sample(at, total) => {
                    // Streams may leave out @S: CD rate then.
                    let rate = if self.rate > 0 { self.rate } else { 44_100 } as f32;
                    if let Some(now) = self.now.as_mut() {
                        now.position = at as f32 / rate;
                        // A stream's total is often unknown (or a frame count).
                        if total > at {
                            now.duration = total as f32 / rate;
                        }
                    }
                }
                Event::State(2) => {
                    self.loading = false;
                    if let Some(now) = self.now.as_mut() {
                        now.status = Status::Playing;
                    }
                }
                Event::State(1) => {
                    if let Some(now) = self.now.as_mut() {
                        now.status = Status::Paused;
                    }
                }
                Event::State(_) => {
                    if !self.loading
                        && self
                            .now
                            .as_ref()
                            .is_some_and(|n| n.status == Status::Playing)
                    {
                        ended = true;
                    }
                }
                Event::Error(message) => {
                    self.loading = false;
                    if let Some(now) = self.now.as_mut() {
                        now.error = Some(message);
                        now.status = Status::Paused;
                    }
                }
            }
        }
        if ended {
            let last = self
                .now
                .as_ref()
                .is_some_and(|n| n.at + 1 >= n.order.len() && !n.repeat);
            if last {
                // The album is over: back to its first song, paused.
                if let Some(now) = self.now.as_mut() {
                    now.at = 0;
                    now.position = 0.;
                    now.status = Status::Paused;
                }
                self.quit();
            } else {
                self.next();
            }
            changed = true;
        }
        if self.playing() && self.polled.is_none_or(|t| t.elapsed() >= POLL) {
            self.polled = Some(Instant::now());
            self.send("SAMPLE");
            // Between answers the bar moves on its own.
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_mpg123_remote_lines() {
        assert_eq!(parse("@P 2"), Some(Event::State(2)));
        assert_eq!(
            parse("@S 1.0 3 44100 Joint-Stereo 2 835 2 0 0 0 256 0 1"),
            Some(Event::Rate(44100))
        );
        assert_eq!(
            parse("@SAMPLE 249984 1090"),
            Some(Event::Sample(249984, 1090))
        );
        assert_eq!(
            parse("@E Error opening stream"),
            Some(Event::Error("Error opening stream".into()))
        );
        assert_eq!(parse("@F 1 2 0.02 3.0"), None);
        assert_eq!(parse("@I ID3v2.title:In"), None);
    }

    #[test]
    fn shuffle_starts_with_the_chosen_song_and_keeps_every_song() {
        let order = shuffled(12, 5);
        assert_eq!(order[0], 5);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..12).collect::<Vec<_>>());
    }

    fn engine(songs: usize) -> Engine {
        let mut e = Engine::default();
        e.now = Some(NowPlaying {
            album_id: 1,
            album: "A".into(),
            artist: "B".into(),
            cover: None,
            songs: (0..songs)
                .map(|i| MusicSong {
                    title: format!("{i}"),
                    artist: String::new(),
                    url: String::new(),
                    item: String::new(),
                    ms: 1000,
                })
                .collect(),
            order: (0..songs).collect(),
            at: 0,
            shuffle: false,
            repeat: false,
            status: Status::Paused,
            position: 0.,
            duration: 0.,
            error: None,
        });
        e
    }

    #[test]
    fn shuffle_off_returns_to_album_order_at_the_same_song() {
        let mut e = engine(8);
        e.toggle_shuffle();
        let song = e.now.as_ref().unwrap().song();
        e.toggle_shuffle();
        let now = e.now.as_ref().unwrap();
        assert_eq!((now.song(), now.at), (song, song));
        assert_eq!(now.order, (0..8).collect::<Vec<_>>());
    }
}
