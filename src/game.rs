//! Minimal Sunshine game card, separate from the media movie popup.
use super::Target;
use crate::stacks::StackItem;
#[derive(Clone, Debug)]
pub struct GameCard {
    pub id: i64,
    pub title: String,
    pub poster: Option<String>,
    pub hdr: bool,
    pub host: String,
    pub online: bool,
    pub restore: Target,
    /// Apollo reports this game as running: offer Resume / Close game.
    pub running: bool,
    pub opened: std::time::Instant,
    /// Content comes in when Steam answered or GAME_WAIT ran out.
    pub shown: Option<std::time::Instant>,
    /// The key art fades in when it is there.
    pub backdrop_at: Option<std::time::Instant>,
    /// Full-screen screenshot viewer: which one is open.
    pub shot: Option<usize>,
}
/// Steam answers from the disk cache at once; a first lookup can take longer.
pub const GAME_WAIT: f32 = 1.6;
impl GameCard {
    pub fn new(item: &StackItem, host: &str, online: bool, hdr: bool, restore: Target) -> Self {
        Self {
            id: item.id,
            title: item.title.clone(),
            poster: item.poster.clone(),
            hdr,
            host: host.to_owned(),
            online,
            restore,
            running: false,
            opened: std::time::Instant::now(),
            shown: None,
            backdrop_at: None,
            shot: None,
        }
    }
}
