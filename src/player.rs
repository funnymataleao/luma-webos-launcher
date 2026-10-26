//! Source-driven cinema player. The UI owns no decoder or network credentials.
//! Native playback is delegated to the webOS uMediaServer bridge on ARM.
use crate::state::{Direction, Target};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Movie,
    Series,
    Youtube,
    Local,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    pub start_ms: u64,
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

fn parse_stamp(raw: &str) -> Option<u64> {
    let hms: Vec<_> = raw.trim().split(':').collect();
    if hms.len() < 2 || hms.len() > 3 {
        return None;
    }
    let fraction = hms.last()?.replace(',', ".");
    let (sec, dec) = fraction.split_once('.').unwrap_or((&fraction, "0"));
    let seconds = sec.parse::<u64>().ok()?;
    let cent = dec.chars().take(3).collect::<String>();
    let ms = format!("{cent:0<3}").parse::<u64>().ok()?;
    let min = hms.get(hms.len() - 2)?.parse::<u64>().ok()?;
    let hour = if hms.len() == 3 {
        hms[0].parse::<u64>().ok()?
    } else {
        0
    };
    Some(((hour * 60 + min) * 60 + seconds) * 1000 + ms)
}
/// SRT/WebVTT + plain ASS dialogue fallback. No markup executes in the UI.
pub fn parse_subtitle(input: &str) -> Vec<Cue> {
    let text = input.replace("\r\n", "\n");
    let mut cues = Vec::new();
    for part in text.split("\n\n") {
        let lines: Vec<&str> = part.lines().collect();
        if let Some((i, timing)) = lines
            .iter()
            .enumerate()
            .find(|(_, line)| line.contains(" --> "))
        {
            let (a, b) = timing.split_once(" --> ").unwrap();
            let b = b.split_whitespace().next().unwrap_or("");
            if let (Some(start_ms), Some(end_ms)) = (parse_stamp(a), parse_stamp(b)) {
                let body = lines
                    .iter()
                    .skip(i + 1)
                    .copied()
                    .collect::<Vec<_>>()
                    .join("\n");
                let body = body
                    .replace("<i>", "")
                    .replace("</i>", "")
                    .replace("<b>", "")
                    .replace("</b>", "");
                if !body.trim().is_empty() {
                    cues.push(Cue {
                        start_ms,
                        end_ms,
                        text: body,
                    });
                }
            }
        }
    }
    if cues.is_empty() {
        for line in text.lines().filter(|l| l.starts_with("Dialogue:")) {
            let fields: Vec<_> = line[9..].splitn(10, ',').collect();
            if fields.len() != 10 {
                continue;
            }
            let (Some(start_ms), Some(end_ms)) = (parse_stamp(fields[1]), parse_stamp(fields[2]))
            else {
                continue;
            };
            let body = fields[9].replace(r"\N", "\n");
            let mut clean = String::new();
            let mut inside = false;
            for c in body.chars() {
                if c == '{' {
                    inside = true;
                } else if c == '}' {
                    inside = false;
                } else if !inside {
                    clean.push(c);
                }
            }
            if !clean.trim().is_empty() {
                cues.push(Cue {
                    start_ms,
                    end_ms,
                    text: clean,
                });
            }
        }
    }
    cues.sort_by_key(|c| c.start_ms);
    cues.truncate(20000);
    cues
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub name: String,
    pub url: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quality {
    pub name: String,
    pub url: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    pub url: String,
    pub title: String,
    pub source: Source,
    pub resume_ms: u64,
    pub duration_ms: u64,
    pub poster: Option<String>,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
    pub qualities: Vec<Quality>,
    pub chapters: Vec<Chapter>,
    pub item_id: Option<i64>,
    pub episode_line: Option<String>,
    pub prev_episode: Option<(i64, i64)>,
    pub next_episode: Option<(i64, i64)>,
    pub selected_subtitle: Option<usize>,
    /// Media server (Jellyfin) the web player reports progress to. Never logged.
    pub report: Option<Report>,
    /// An album: the web player shows its "now playing" screen instead of video.
    pub music: Option<Music>,
}

/// Songs the web player plays one after another, with the album's cover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Music {
    pub album: String,
    pub artist: String,
    pub year: i64,
    /// Image URL the player loads itself (no token needed).
    pub cover: String,
    pub songs: Vec<MusicSong>,
    pub start: usize,
    pub shuffle: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusicSong {
    pub title: String,
    pub artist: String,
    /// Stream URL; carries the token, never logged.
    pub url: String,
    pub item: String,
    pub ms: u64,
}

/// Where the web player sends playback progress, and the series' other
/// episodes it can switch to without asking Luma. Holds the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    /// "jellyfin".
    pub service: String,
    pub base: String,
    pub token: String,
    pub device_id: String,
    pub item: String,
    pub source: String,
    pub session: String,
    pub episodes: Vec<ReportEpisode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportEpisode {
    pub season: i64,
    pub number: i64,
    pub item: String,
    pub url: String,
    pub line: String,
}
impl OpenRequest {
    pub fn sanitize(mut self) -> Result<Self, &'static str> {
        if !self.url.starts_with("https://")
            && !self.url.starts_with("http://")
            && !self.url.starts_with("file://")
        {
            return Err("Unsupported player URL scheme");
        }
        self.chapters.sort_by_key(|c| c.start_ms);
        self.chapters.dedup_by_key(|c| c.start_ms);
        self.resume_ms = self.resume_ms.min(self.duration_ms.max(self.resume_ms));
        Ok(self)
    }
    pub fn preview(source: Source) -> Self {
        let chapters = if matches!(source, Source::Local | Source::Youtube) {
            vec![
                Chapter {
                    title: "Opening".into(),
                    start_ms: 0,
                    thumbnail: None,
                },
                Chapter {
                    title: "The journey".into(),
                    start_ms: 840_000,
                    thumbnail: None,
                },
                Chapter {
                    title: "Final act".into(),
                    start_ms: 3_100_000,
                    thumbnail: None,
                },
            ]
        } else {
            vec![]
        };
        Self {
            url: "https://example.invalid/demo.mp4".into(),
            title: "Player preview".into(),
            source,
            resume_ms: 1_620_000,
            duration_ms: 5_420_000,
            poster: None,
            audio: vec![
                Track {
                    name: "Russian · HDRezka".into(),
                    url: None,
                },
                Track {
                    name: "English · Original".into(),
                    url: None,
                },
            ],
            subtitles: vec![
                Track {
                    name: "English".into(),
                    url: None,
                },
                Track {
                    name: "Português".into(),
                    url: None,
                },
            ],
            qualities: vec![
                Quality {
                    name: "4K HDR".into(),
                    url: None,
                },
                Quality {
                    name: "1080p".into(),
                    url: None,
                },
            ],
            chapters,
            item_id: (source == Source::Series).then_some(1),
            episode_line: (source == Source::Series)
                .then(|| "Season 1 · Episode 3 · The Bridge".into()),
            prev_episode: (source == Source::Series).then_some((1, 2)),
            next_episode: (source == Source::Series).then_some((1, 4)),
            selected_subtitle: Some(0),
            report: None,
            music: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Audio,
    Subtitles,
    Quality,
    Chapters,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open(Box<OpenRequest>),
    Pause,
    Play,
    Seek(u64),
    Audio(usize),
    Subtitle(Option<usize>),
    Quality(usize),
    Close,
    Episode(i64, i64, i64),
}
#[derive(Debug)]
pub struct Session {
    pub request: OpenRequest,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub buffered_ms: u64,
    pub paused: bool,
    pub ready: bool,
    pub first_frame_ms: Option<u64>,
    pub error: Option<String>,
    pub panel: Option<Panel>,
    pub selected_audio: usize,
    pub selected_subtitle: Option<usize>,
    pub selected_quality: usize,
    pub selected_row: usize,
    pub selected_pill: usize,
    pub controls_since: Instant,
    pub hover_seek_ms: Option<u64>,
    pub cues: Vec<Cue>,
}
impl Session {
    pub fn new(request: OpenRequest) -> Self {
        let position_ms = request.resume_ms;
        let duration_ms = request.duration_ms;
        let now = Instant::now();
        let selected_subtitle = request.selected_subtitle;
        Self {
            request,
            position_ms,
            duration_ms,
            buffered_ms: 0,
            paused: false,
            ready: false,
            first_frame_ms: None,
            error: None,
            panel: None,
            selected_audio: 0,
            selected_subtitle,
            selected_quality: 0,
            selected_row: 0,
            selected_pill: 0,
            controls_since: now,
            hover_seek_ms: None,
            cues: vec![],
        }
    }
    pub fn reveal(&mut self) {
        self.controls_since = Instant::now();
    }
    pub fn controls_visible(&self) -> bool {
        self.panel.is_some()
            || self.paused
            || !self.ready
            || self.controls_since.elapsed() < Duration::from_secs(4)
    }
    pub fn pills(&self) -> Vec<Panel> {
        let mut out = Vec::new();
        if !self.request.audio.is_empty() {
            out.push(Panel::Audio);
        }
        if !self.request.subtitles.is_empty() {
            out.push(Panel::Subtitles);
        }
        if !self.request.qualities.is_empty() {
            out.push(Panel::Quality);
        }
        if !self.request.chapters.is_empty() {
            out.push(Panel::Chapters);
        }
        out
    }
    /// Bottom row in visual order: track pills, then the right-hand group.
    pub fn bottom_row(&self) -> Vec<Target> {
        let mut row: Vec<Target> = (0..self.pills().len()).map(Target::PlayerPill).collect();
        if self.request.source == Source::Series {
            if self.request.prev_episode.is_some() {
                row.push(Target::PlayerPreviousEpisode);
            }
            if self.request.next_episode.is_some() {
                row.push(Target::PlayerNextEpisode);
            }
        } else {
            row.push(Target::PlayerRestart);
        }
        row
    }
    pub fn pill_name(&self, p: Panel) -> String {
        match p {
            Panel::Audio => self
                .request
                .audio
                .get(self.selected_audio)
                .map_or("Audio".into(), |a| a.name.clone()),
            Panel::Subtitles => self
                .selected_subtitle
                .and_then(|i| self.request.subtitles.get(i))
                .map_or("Subtitles · Off".into(), |t| t.name.clone()),
            Panel::Quality => self
                .request
                .qualities
                .get(self.selected_quality)
                .map_or("Auto".into(), |q| q.name.clone()),
            Panel::Chapters => "Chapters".into(),
        }
    }
    pub fn panel_rows(&self, p: Panel) -> Vec<String> {
        match p {
            Panel::Audio => self.request.audio.iter().map(|t| t.name.clone()).collect(),
            Panel::Subtitles => std::iter::once("Off".into())
                .chain(self.request.subtitles.iter().map(|t| t.name.clone()))
                .collect(),
            Panel::Quality => self
                .request
                .qualities
                .iter()
                .map(|q| q.name.clone())
                .collect(),
            Panel::Chapters => self
                .request
                .chapters
                .iter()
                .map(|c| format!("{} · {}", clock(c.start_ms), c.title))
                .collect(),
        }
    }
    pub fn current_caption(&self) -> Option<&str> {
        self.cues
            .iter()
            .find(|c| c.start_ms <= self.position_ms && c.end_ms > self.position_ms)
            .map(|c| c.text.as_str())
    }
    pub fn current_chapter(&self) -> Option<&Chapter> {
        self.request
            .chapters
            .iter()
            .rev()
            .find(|c| c.start_ms <= self.position_ms)
    }
    pub fn seek_by(&mut self, delta: i64) -> Command {
        let end = self.duration_ms.max(self.position_ms).max(1);
        self.position_ms = self
            .position_ms
            .saturating_add_signed(delta)
            .min(end.saturating_sub(1));
        self.reveal();
        Command::Seek(self.position_ms)
    }
    pub fn navigate(&mut self, direction: Direction) -> Vec<Command> {
        self.reveal();
        if let Some(panel) = self.panel {
            let n = self.panel_rows(panel).len();
            if panel == Panel::Chapters {
                // Chapters are a horizontal strip of cards.
                match direction {
                    Direction::Left => self.selected_row = self.selected_row.saturating_sub(1),
                    Direction::Right => {
                        self.selected_row = (self.selected_row + 1).min(n.saturating_sub(1))
                    }
                    Direction::Up | Direction::Down => self.panel = None,
                }
                return vec![];
            }
            match direction {
                Direction::Up => self.selected_row = self.selected_row.saturating_sub(1),
                Direction::Down => {
                    self.selected_row = (self.selected_row + 1).min(n.saturating_sub(1))
                }
                Direction::Left => {
                    self.panel = None;
                    self.selected_row = 0;
                }
                Direction::Right => {}
            }
            return vec![];
        }
        let n = self.pills().len();
        match direction {
            Direction::Left => vec![self.seek_by(-10_000)],
            Direction::Right => vec![self.seek_by(10_000)],
            Direction::Down => {
                self.selected_pill = (self.selected_pill + 1).min(n.saturating_sub(1));
                vec![]
            }
            Direction::Up => {
                self.selected_pill = self.selected_pill.saturating_sub(1);
                vec![]
            }
        }
    }
    pub fn activate(&mut self, target: Option<&Target>) -> Vec<Command> {
        self.reveal();
        if let Some(Target::PlayerClose) = target {
            return vec![Command::Close];
        }
        if let Some(Target::PlayerPanelClose) = target {
            self.panel = None;
            return vec![];
        }
        if let Some(Target::PlayerPreviousEpisode | Target::PlayerNextEpisode) = target {
            let episode = if matches!(target, Some(Target::PlayerPreviousEpisode)) {
                self.request.prev_episode
            } else {
                self.request.next_episode
            };
            return episode
                .and_then(|(season, number)| {
                    self.request
                        .item_id
                        .map(|id| Command::Episode(id, season, number))
                })
                .into_iter()
                .collect();
        }
        if let Some(Target::PlayerChapter(i)) = target
            && let Some(chapter) = self.request.chapters.get(*i)
        {
            self.position_ms = chapter.start_ms;
            self.panel = None;
            return vec![Command::Seek(chapter.start_ms)];
        }
        if let Some(Target::PlayerSeek) = target {
            return vec![];
        }
        if let Some(Target::PlayerRestart) = target {
            self.position_ms = 0;
            return vec![Command::Seek(0)];
        }
        if let Some(Target::PlayerPill(i)) = target {
            self.selected_pill = *i;
        }
        if let Some(
            Target::PlayerOption(i)
            | Target::PlayerAudioOption(i)
            | Target::PlayerSubtitleOption(i),
        ) = target
        {
            self.selected_row = *i;
        }
        if let Some(panel) = self.panel {
            let i = self.selected_row;
            let cmd = match panel {
                Panel::Audio => {
                    self.selected_audio = i;
                    Some(Command::Audio(i))
                }
                Panel::Subtitles => {
                    self.selected_subtitle = i.checked_sub(1);
                    Some(Command::Subtitle(self.selected_subtitle))
                }
                Panel::Quality => {
                    self.selected_quality = i;
                    Some(Command::Quality(i))
                }
                Panel::Chapters => self
                    .request
                    .chapters
                    .get(i)
                    .map(|c| Command::Seek(c.start_ms)),
            };
            if let Some(Command::Seek(ms)) = cmd.as_ref() {
                self.position_ms = *ms;
            }
            self.panel = None;
            return cmd.into_iter().collect();
        }
        if matches!(target, Some(Target::PlayerPill(_))) {
            if let Some(p) = self.pills().get(self.selected_pill).copied() {
                self.panel = Some(p);
                self.selected_row = match p {
                    Panel::Audio => self.selected_audio,
                    Panel::Subtitles => self.selected_subtitle.map_or(0, |i| i + 1),
                    Panel::Quality => self.selected_quality,
                    Panel::Chapters => self
                        .request
                        .chapters
                        .iter()
                        .position(|c| {
                            self.current_chapter()
                                .is_some_and(|cur| cur.start_ms == c.start_ms)
                        })
                        .unwrap_or(0),
                };
            }
            return vec![];
        }
        self.paused = !self.paused;
        vec![if self.paused {
            Command::Pause
        } else {
            Command::Play
        }]
    }
    pub fn back(&mut self) -> Vec<Command> {
        self.reveal();
        if self.panel.take().is_some() {
            vec![]
        } else {
            vec![Command::Close]
        }
    }
}
pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s < 3600 {
        return format!("{}:{:02}", s / 60, s % 60);
    }
    format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_chapters() {
        let s = Session::new(OpenRequest::preview(Source::Local));
        assert_eq!(s.pills().len(), 4);
        assert_eq!(s.current_chapter().unwrap().title, "The journey");
    }
    #[test]
    fn media_hides_chapters() {
        let s = Session::new(OpenRequest::preview(Source::Movie));
        assert_eq!(s.pills().len(), 3);
        assert!(s.current_chapter().is_none());
    }
    #[test]
    fn panel_back_first_then_close() {
        let mut s = Session::new(OpenRequest::preview(Source::Youtube));
        s.selected_pill = 3;
        s.activate(Some(&Target::PlayerPill(3)));
        assert_eq!(s.panel, Some(Panel::Chapters));
        assert_eq!(s.back(), vec![]);
        assert_eq!(s.back(), vec![Command::Close]);
    }
    #[test]
    fn subtitle_off_and_quality() {
        let mut s = Session::new(OpenRequest::preview(Source::Movie));
        s.selected_pill = 1;
        s.activate(Some(&Target::PlayerPill(1)));
        s.selected_row = 2;
        assert_eq!(s.activate(None), vec![Command::Subtitle(Some(1))]);
        s.selected_pill = 2;
        s.activate(Some(&Target::PlayerPill(2)));
        s.selected_row = 1;
        assert_eq!(s.activate(None), vec![Command::Quality(1)]);
    }
    #[test]
    fn caption_formats() {
        assert_eq!(
            parse_subtitle("1\n00:00:01,100 --> 00:00:03,500\nHi there\n\n")[0].text,
            "Hi there"
        );
        assert_eq!(
            parse_subtitle("WEBVTT\n\n00:02.200 --> 00:04.300\nHello")[0].start_ms,
            2200
        );
        assert_eq!(
            parse_subtitle(
                "[Events]\nDialogue: 0,0:00:11.23,0:00:12.70,Default,,0,0,0,,Hi {\\i1}world"
            )[0]
            .text,
            "Hi world"
        );
    }
    #[test]
    fn clamps_seek() {
        let mut s = Session::new(OpenRequest::preview(Source::Movie));
        assert_eq!(s.seek_by(-9_000_000), Command::Seek(0));
        assert_eq!(s.seek_by(9_000_000), Command::Seek(5_419_999));
    }
}
