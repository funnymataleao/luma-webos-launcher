//! Home mini-player (background music, mockup 26.10): a glass pill under the
//! header circles while an album plays. Down from a circle enters it, Left
//! and Right walk its buttons, OK on the cover opens the album card.
use super::{Control, Direction, Effect, State, Target};

/// Left to right.
pub const ROW: [Target; 6] = [
    Target::MiniCover,
    Target::MiniShuffle,
    Target::MiniPrev,
    Target::MiniPlay,
    Target::MiniNext,
    Target::MiniRepeat,
];

fn header(target: &Target) -> bool {
    matches!(target, Target::TvSettings | Target::Input(_))
}

impl State {
    /// Remote arrows on and into the mini-player; None leaves the press to
    /// the rest of Home.
    pub fn mini_direction(
        &mut self,
        direction: Direction,
        controls: &[Control],
    ) -> Option<Vec<Effect>> {
        if !controls.iter().any(|c| c.target == Target::MiniPlay) {
            return None;
        }
        if header(&self.focus) && matches!(direction, Direction::Down) {
            self.focus(Target::MiniPlay, controls);
            return Some(vec![]);
        }
        let at = ROW.iter().position(|t| *t == self.focus)?;
        match direction {
            Direction::Left if at > 0 => self.focus(ROW[at - 1].clone(), controls),
            Direction::Right if at + 1 < ROW.len() => self.focus(ROW[at + 1].clone(), controls),
            Direction::Left | Direction::Right => {}
            Direction::Up | Direction::Down => {
                // The circle above, nearest to the button.
                let x = controls
                    .iter()
                    .find(|c| c.target == self.focus)
                    .map_or(1800., |c| c.rect.center().0);
                let circle = controls
                    .iter()
                    .filter(|c| header(&c.target))
                    .min_by(|a, b| {
                        (a.rect.center().0 - x)
                            .abs()
                            .total_cmp(&(b.rect.center().0 - x).abs())
                    })
                    .map(|c| c.target.clone())?;
                self.focus(circle, controls);
                if matches!(direction, Direction::Down) {
                    // Down goes on from the circle as if the pill were not there.
                    return None;
                }
            }
        }
        Some(vec![])
    }

    /// OK on the mini-player; None for other targets.
    pub fn mini_activate(&mut self) -> Option<Vec<Effect>> {
        match self.focus {
            Target::MiniPlay => self.music.toggle(),
            Target::MiniPrev => self.music.prev(),
            Target::MiniNext => self.music.next(),
            Target::MiniShuffle => self.music.toggle_shuffle(),
            Target::MiniRepeat => self.music.toggle_repeat(),
            Target::MiniCover => {
                let now = self.music.now.as_ref()?;
                let item = crate::stacks::StackItem {
                    title: now.album.clone(),
                    poster: now.cover.clone(),
                    id: now.album_id,
                };
                return Some(self.open_album(item, Target::MiniCover));
            }
            _ => return None,
        }
        Some(vec![])
    }

    /// The music engine has news: redraw, and never leave focus on a pill
    /// that went away.
    pub fn music_changed(&mut self) {
        self.dirty = true;
        if self.music.now.is_none() && ROW.contains(&self.focus) {
            self.focus = Target::TvSettings;
        }
    }
}
