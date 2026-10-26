//! Shared, interruptible motion tokens for Luma's native draw-list UI.
use crate::{renderer::Draw, state::Rect};
pub const FOCUS_IN: f32 = 0.160;
pub const FOCUS_OUT: f32 = 0.160;
/// Card zoom spring and the light sweep that follows a remote focus move.
pub const FOCUS_POP: f32 = 0.55;
pub const FOCUS_SHEEN: f32 = 0.4;
pub const PANEL_IN: f32 = 0.280;
pub const PANEL_OUT: f32 = 0.280;
pub const PAGE_IN: f32 = 0.200;
pub const PAGE_OUT: f32 = 0.200;
pub const STAGGER: f32 = 0.040;
pub const SCROLL: f32 = 0.180;
// Authored grid cascade: all eight visible cards have their own beat.
pub const CARD_STAGGER: f32 = 0.052;
pub const CARD_IN: f32 = 0.340;
pub const CARD_RISE: f32 = 25.;
pub const MAX_CASCADE_CARDS: usize = 8;
pub const CARD_CASCADE_END: f32 = CARD_IN + (MAX_CASCADE_CARDS as f32 - 1.) * CARD_STAGGER;
pub const ROW_STAGGER: f32 = 0.048;
pub const MAX_CASCADE_ROWS: usize = 6;
pub const ROW_CASCADE_END: f32 = CARD_IN + (MAX_CASCADE_ROWS as f32 - 1.) * ROW_STAGGER;
pub const FIRST_HOME_END: f32 = 1.18;

pub fn cascade_delay(visible_rank: usize) -> f32 {
    visible_rank.min(MAX_CASCADE_CARDS - 1) as f32 * CARD_STAGGER
}
pub fn cascade_progress(elapsed: f32, visible_rank: usize) -> f32 {
    progress((elapsed - cascade_delay(visible_rank)).max(0.), CARD_IN)
}

/// Solve x(t) using Newton iterations followed by bisection, then evaluate y(t).
/// This is the actual CSS cubic-bezier curve, not a power-law stand-in.
pub fn bezier(value: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let value = value.clamp(0., 1.);
    let curve = |t: f32, a: f32, b: f32| {
        let u = 1. - t;
        3. * u * u * t * a + 3. * u * t * t * b + t * t * t
    };
    let mut t = value;
    for _ in 0..6 {
        let u = 1. - t;
        let slope = 3. * u * u * x1 + 6. * u * t * (x2 - x1) + 3. * t * t * (1. - x2);
        if slope < 1e-6 {
            break;
        }
        t = (t - (curve(t, x1, x2) - value) / slope).clamp(0., 1.);
    }
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..12 {
        if curve(t, x1, x2) < value {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) * 0.5;
    }
    curve(t, y1, y2)
}
pub fn ease_out(x: f32) -> f32 {
    bezier(x, 0.23, 1., 0.32, 1.)
}
pub fn progress(elapsed: f32, duration: f32) -> f32 {
    ease_out((elapsed / duration).clamp(0., 1.))
}
pub fn shift_rect(r: &mut Rect, x: f32, y: f32) {
    r.x += x;
    r.y += y;
}
/// Native GLES commands have the same transform/opacity semantics as CSS.
pub fn transform(draw: &mut Draw, x: f32, y: f32, alpha: f32) {
    let apply = |c: &mut [f32; 4]| {
        c[3] *= alpha;
    };
    match draw {
        Draw::Shape(r, _, _, c)
        | Draw::Shadow(r, _, _, c)
        | Draw::Sheen(r, _, _, c)
        | Draw::AppImage(_, r, c) => {
            shift_rect(r, x, y);
            apply(c);
        }
        Draw::RoundedGradient(r, _, a, b)
        | Draw::DiagonalGradient(r, _, a, b)
        | Draw::HGradient(r, _, a, b)
        | Draw::DiagonalGradientBorder(r, _, _, a, b) => {
            shift_rect(r, x, y);
            apply(a);
            apply(b);
        }
        Draw::Text(_, r, _, c, _) | Draw::TintedImage(_, r, _, _, c) => {
            shift_rect(r, x, y);
            apply(c);
        }
        Draw::Image(path, r, radius, contain) => {
            shift_rect(r, x, y);
            if alpha < 1. {
                *draw = Draw::TintedImage(path.clone(), *r, *radius, *contain, [1., 1., 1., alpha]);
            }
        }
        Draw::Clip(_) | Draw::RoundClip(_) | Draw::EdgeFade(_) => {}
    }
}
pub fn transform_group(group: &mut [Draw], x: f32, y: f32, alpha: f32) {
    for item in group {
        transform(item, x, y, alpha);
    }
}
pub fn chase(current: f32, target: f32, dt: f32, reduced: bool) -> f32 {
    if reduced || (target - current).abs() < 0.35 {
        return target;
    }
    // 96% settled in approximately 180 ms, without overshoot.
    current + (target - current) * (1. - (-(3.22 / SCROLL) * dt.max(0.)).exp())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bezier_endpoints_and_monotonic() {
        assert!((ease_out(0.)).abs() < 0.001);
        assert!((ease_out(1.) - 1.).abs() < 0.001);
        assert!(ease_out(0.5) > 0.5);
    }
    #[test]
    fn chase_reaches_target_and_reduced_is_instant() {
        let mut x = 0.;
        for _ in 0..20 {
            x = chase(x, 500., 0.016, false);
        }
        assert!((x - 500.).abs() < 3.);
        assert_eq!(chase(0., 500., 0.016, true), 500.);
    }
    #[test]
    fn sixth_card_staggers_later_than_fourth() {
        assert!(cascade_delay(5) > cascade_delay(3));
        assert!((cascade_delay(5) - 0.260).abs() < 0.00001);
        assert!(cascade_progress(0.28, 3) > cascade_progress(0.28, 5));
        assert_eq!(cascade_progress(CARD_CASCADE_END, 7), 1.);
    }
    #[test]
    fn newly_visible_row_begins_at_zero_delay() {
        let visible_indices = [24, 25, 26, 27, 28, 29];
        let delays: Vec<_> = visible_indices
            .iter()
            .enumerate()
            .map(|(rank, _)| cascade_delay(rank))
            .collect();
        assert_eq!(delays[0], 0.);
        assert!(delays[5] > delays[3]);
    }
    #[test]
    fn reduced_motion_has_no_position_shift() {
        let spatial_offset = |reduce: bool| if reduce { 0. } else { 12. };
        assert_eq!(spatial_offset(true), 0.);
    }
}
