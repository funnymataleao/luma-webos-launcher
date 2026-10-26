//! Animated illustrations for the Settings preview panel. Each section draws
//! one scene on a 636×548 design stage that is scaled into the free space of
//! the panel. Every loop runs on one shared clock, so moving focus between
//! rows never restarts an animation, and Reduce Motion shows a still frame.
use super::*;
use crate::model::ACCENT_GRADIENTS;
use crate::state::{APPEARANCE_SECTION, INPUTS_SECTION, SMARTHOME_SECTION, STACKS_SECTION};
use std::{
    f32::consts::{PI, TAU},
    hash::{Hash, Hasher},
    sync::OnceLock,
    time::Instant,
};

pub(super) const STAGE_W: f32 = 636.;
pub(super) const STAGE_H: f32 = 548.;

thread_local! {
    static NOW: Cell<Option<OffsetDateTime>> = const { Cell::new(None) };
}
/// The frame's wall clock, so the General clock matches the header.
pub(super) fn set_now(now: OffsetDateTime) {
    NOW.with(|n| n.set(Some(now)));
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Art {
    General,
    Wallpaper,
    Weather,
    Inputs,
    SmartHome,
    System,
    Appearance,
    Stacks,
}
impl Art {
    pub(super) fn for_section(section: usize) -> Self {
        match section {
            1 => Self::Wallpaper,
            WEATHER_SECTION => Self::Weather,
            INPUTS_SECTION => Self::Inputs,
            SMARTHOME_SECTION => Self::SmartHome,
            SYSTEM_SECTION => Self::System,
            APPEARANCE_SECTION => Self::Appearance,
            STACKS_SECTION => Self::Stacks,
            _ => Self::General,
        }
    }
    /// A calm, representative frame for Reduce Motion.
    fn still(self) -> f64 {
        match self {
            Self::General => 0.8,
            Self::Wallpaper => 2.,
            Self::Weather => 1.,
            Self::Inputs => 3.4,
            Self::SmartHome => 5.8,
            Self::System => 1.5,
            Self::Appearance => 1.,
            Self::Stacks => 0.,
        }
    }
}

/// Identity of the preview content. The crossfade compares keys, because an
/// animated scene differs from its own previous frame every time.
pub(super) fn key(art: Art, name: &str, detail: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (art, name, detail).hash(&mut h);
    h.finish() | 1
}

fn seconds() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}
fn phase(t: f64, period: f64) -> f32 {
    (t.rem_euclid(period) / period) as f32
}
/// CSS `alternate` ease-in-out: 0 → 1 → 0 over two periods.
fn swing(t: f64, period: f64) -> f32 {
    let p = (t.rem_euclid(2. * period) / period) as f32;
    smooth(if p > 1. { 2. - p } else { p })
}
fn smooth(u: f32) -> f32 {
    let u = u.clamp(0., 1.);
    u * u * (3. - 2. * u)
}
fn linear(u: f32) -> f32 {
    u.clamp(0., 1.)
}
fn snappy(u: f32) -> f32 {
    motion::bezier(u, 0.22, 1., 0.36, 1.)
}
fn ease_in(u: f32) -> f32 {
    motion::bezier(u, 0.42, 0., 1., 1.)
}
/// Keyframes over one loop: (position 0..1, value), each segment eased.
fn keys(p: f32, k: &[(f32, f32)], ease: fn(f32) -> f32) -> f32 {
    if p <= k[0].0 {
        return k[0].1;
    }
    for w in k.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if p <= t1 {
            let u = if t1 > t0 { (p - t0) / (t1 - t0) } else { 1. };
            return v0 + (v1 - v0) * ease(u);
        }
    }
    k[k.len() - 1].1
}
/// Crossfade weights as "over" alphas: each layer is drawn on top of the
/// previous ones, so two half-faded layers never dip in brightness.
fn over_alphas<const N: usize>(k: [f32; N]) -> [f32; N] {
    let mut acc = 0.;
    k.map(|v| {
        acc += v;
        if acc > 0. { v / acc } else { 0. }
    })
}

/// Draw commands in stage coordinates.
struct Pen<'a> {
    out: &'a mut Scene,
    assets: &'a Assets,
    x: f32,
    y: f32,
    s: f32,
}
impl Pen<'_> {
    /// A rect in stage units, mapped to the screen.
    fn r(&self, r: Rect) -> Rect {
        Rect::new(
            self.x + r.x * self.s,
            self.y + r.y * self.s,
            r.w * self.s,
            r.h * self.s,
        )
    }
    fn fill(&mut self, r: Rect, radius: f32, color: Color) {
        let r = self.r(r);
        self.out.shape(r, radius * self.s, color);
    }
    fn stroke(&mut self, r: Rect, radius: f32, width: f32, color: Color) {
        let r = self.r(r);
        self.out.border(r, radius * self.s, width * self.s, color);
    }
    fn disc(&mut self, cx: f32, cy: f32, d: f32, color: Color) {
        self.fill(Rect::new(cx - d / 2., cy - d / 2., d, d), d / 2., color);
    }
    fn grad(&mut self, r: Rect, radius: f32, a: Color, b: Color) {
        let r = self.r(r);
        self.out
            .draw
            .push(Draw::DiagonalGradient(r, radius * self.s, a, b));
    }
    fn vgrad(&mut self, r: Rect, radius: f32, a: Color, b: Color) {
        let r = self.r(r);
        self.out
            .draw
            .push(Draw::RoundedGradient(r, radius * self.s, a, b));
    }
    fn shadow(&mut self, r: Rect, radius: f32, blur: f32, alpha: f32) {
        let r = self.r(r);
        self.out
            .shadow(r, radius * self.s, blur * self.s, blur * self.s / 3., alpha);
    }
    /// The user's accent (solid or gradient) at `alpha`.
    fn accent(&mut self, state: &State, r: Rect, radius: f32, alpha: f32) {
        if alpha <= 0.001 {
            return;
        }
        let (a, b) = state.accent_colors();
        if state.preferences.accent.kind == "gradient" {
            self.grad(r, radius, c(a, alpha), c(b, alpha));
        } else {
            self.fill(r, radius, c(a, alpha));
        }
    }
    fn accent_ring(&mut self, state: &State, r: Rect, radius: f32, width: f32, alpha: f32) {
        if alpha <= 0.001 {
            return;
        }
        let (a, b) = state.accent_colors();
        let b = if state.preferences.accent.kind == "gradient" {
            b
        } else {
            a
        };
        let r = self.r(r);
        self.out.draw.push(Draw::DiagonalGradientBorder(
            r,
            radius * self.s,
            width * self.s,
            c(a, alpha),
            c(b, alpha),
        ));
    }
    fn image(&mut self, path: String, r: Rect, radius: f32, alpha: f32) {
        let r = self.r(r);
        self.out.draw.push(Draw::TintedImage(
            path,
            r,
            radius * self.s,
            false,
            [1., 1., 1., alpha],
        ));
    }
    /// Fixed-size text: the user's font and size preferences restyle the
    /// interface, not the proportions of an illustration.
    fn style(&self, size: f32, weight: u16, display: bool) -> TextStyle {
        let a = self.assets;
        let k = if display {
            a.active_heading.get().scale() * a.heading_pct.get() as f32 / 100.
        } else {
            a.active_text.get().scale() * a.text_pct.get() as f32 / 100.
        };
        TextStyle::new(size * self.s / k.max(0.5))
            .weight(weight)
            .display(display)
            .leading(1.2)
    }
    fn heading(&self, size: f32, weight: u16, font: crate::typography::HeadingFont) -> TextStyle {
        let k = font.scale() * self.assets.heading_pct.get() as f32 / 100.;
        TextStyle::new(size * self.s / k.max(0.5))
            .weight(weight)
            .display(true)
            .heading(font)
            .leading(1.2)
    }
    /// Left-aligned text whose line box is vertically centered on `cy` (stage units).
    fn label(&mut self, value: &str, x: f32, w: f32, cy: f32, style: TextStyle, color: Color) {
        if color[3] > 0.004 {
            let r = self.line_box(x, w, cy, style);
            self.out.text(value, r, style, color, false);
        }
    }
    /// Like `label`, horizontally centered in `w`.
    fn label_centered(
        &mut self,
        value: &str,
        x: f32,
        w: f32,
        cy: f32,
        style: TextStyle,
        color: Color,
    ) {
        if color[3] > 0.004 {
            let r = self.line_box(x, w, cy, style);
            self.out.text(value, r, style, color, true);
        }
    }
    fn line_box(&self, x: f32, w: f32, cy: f32, style: TextStyle) -> Rect {
        let h = style.size * style.line_height;
        Rect::new(
            self.x + x * self.s,
            self.y + cy * self.s - h / 2.,
            w * self.s,
            h + 2.,
        )
    }
    /// Width of `value` in stage units.
    fn width(&self, value: &str, style: TextStyle) -> f32 {
        self.assets.width(value, style) / self.s
    }
    /// Scissor to a horizontal band of the stage; vertical motion of page
    /// transitions never cuts the content.
    fn clip_x(&mut self, x: f32, w: f32) {
        let r = self.r(Rect::new(x, 0., w, 0.));
        self.out
            .draw
            .push(Draw::Clip(Some(Rect::new(r.x, 0., r.w, 1080.))));
    }
    fn unclip(&mut self, state: &State) {
        self.out.draw.push(Draw::Clip(Some(Rect::new(
            0.,
            0.,
            1920.,
            state.main_bottom(),
        ))));
    }
}

/// Draws `art` into `stage`, a rect with the 636×548 design aspect.
pub(super) fn draw(out: &mut Scene, state: &State, assets: &Assets, art: Art, stage: Rect) {
    let t = if state.preferences.reduced_motion {
        art.still()
    } else {
        seconds()
    };
    let mut p = Pen {
        out,
        assets,
        x: stage.x,
        y: stage.y,
        s: stage.w / STAGE_W,
    };
    match art {
        Art::General => general(&mut p, state, t),
        Art::Wallpaper => wallpaper(&mut p, state, assets, t),
        Art::Weather => weather(&mut p, t),
        Art::Inputs => inputs(&mut p, state, t),
        Art::SmartHome => smart_home(&mut p, state, t),
        Art::System => system(&mut p, state, assets, t),
        Art::Appearance => appearance(&mut p, t),
        Art::Stacks => stacks(&mut p, state, assets, t),
    }
}

/// Three wallpaper stills shared by Wallpaper and Stacks, so both scenes reuse
/// the same textures.
fn stills(state: &State, assets: &Assets) -> [String; 3] {
    let n = crate::video_catalog::all().len().max(1);
    let first = state.preferences.video_index.min(n - 1);
    [0, 7, 19].map(|step| assets.path(format!("video-stills/{}.jpg", (first + step) % n)))
}

fn general(p: &mut Pen, state: &State, t: f64) {
    p.fill(Rect::new(78., 40., 480., 468.), 28., c(0xffffff, 0.06));
    p.stroke(Rect::new(78., 40., 480., 468.), 28., 1., c(0xffffff, 0.08));
    let language = state.preferences.language;
    let now = NOW.with(Cell::get);
    // Digits only: an AM/PM suffix would crowd the card.
    let clock = now.map_or_else(
        || "09:41".to_owned(),
        |now| language.clock(now, state.preferences.hour_24, false),
    );
    let clock = clock.split_whitespace().next().unwrap_or("09:41");
    // Hours and minutes stay put; only the colon blinks, once a second.
    let style = p.style(80., 700, true).tracking(-0.02);
    let colon = if t.rem_euclid(2.) < 1. { 1. } else { 0.25 };
    if let Some((head, tail)) = clock.split_once(':') {
        let parts = [head, ":", tail];
        let widths = parts.map(|s| p.width(s, style));
        let mut x = 318. - widths.iter().sum::<f32>() / 2.;
        for (i, part) in parts.iter().enumerate() {
            let alpha = if i == 1 { colon } else { 1. };
            p.label(
                part,
                x,
                widths[i] + 8.,
                140.,
                style,
                c(0xffffff, alpha * 0.96),
            );
            x += widths[i];
        }
    } else {
        p.label_centered(clock, 78., 480., 140., style, WHITE);
    }
    let date = now.map_or_else(|| "Sunday, October 25".to_owned(), |now| language.date(now));
    p.label_centered(
        &date,
        78.,
        480.,
        210.,
        p.style(24., 600, false),
        c(0xffffff, 0.68),
    );
    // Greetings in every Luma language, two seconds each, rising in and out.
    let words = ["Hello", "Hallo", "¡Hola!", "Olá", "你好"];
    let greet = p.style(56., 700, true);
    for (i, word) in words.iter().enumerate() {
        let q = phase(t - 2. * i as f64, 10.);
        let (alpha, dy) = if q < 0.04 {
            let u = smooth(q / 0.04);
            (u, 16. * (1. - u))
        } else if q < 0.17 {
            (1., 0.)
        } else if q < 0.21 {
            let u = smooth((q - 0.17) / 0.04);
            (1. - u, -16. * u)
        } else {
            (0., 0.)
        };
        p.label_centered(word, 78., 480., 272. + dy, greet, c(0xffffff, 0.96 * alpha));
    }
    // The language chip highlight follows the greeting.
    let q = phase(t, 10.);
    let hx = keys(
        q,
        &[
            (0., 0.),
            (0.17, 0.),
            (0.21, 80.),
            (0.37, 80.),
            (0.41, 160.),
            (0.57, 160.),
            (0.61, 240.),
            (0.77, 240.),
            (0.81, 320.),
            (0.97, 320.),
            (1., 0.),
        ],
        snappy,
    );
    p.fill(Rect::new(122. + hx, 340., 72., 48.), 24., c(0xffffff, 0.2));
    let chip = p.style(20., 700, false);
    for (i, code) in ["EN", "DE", "ES", "PT", "中文"].iter().enumerate() {
        p.label_centered(
            code,
            122. + 80. * i as f32,
            72.,
            364.,
            chip,
            c(0xffffff, 0.75),
        );
    }
    // Motion toggle flips on and off.
    p.label("Motion", 138., 220., 452., p.style(26., 600, false), WHITE);
    let on = keys(
        phase(t, 6.),
        &[
            (0., 0.),
            (0.3, 0.),
            (0.38, 1.),
            (0.8, 1.),
            (0.88, 0.),
            (1., 0.),
        ],
        snappy,
    );
    p.fill(Rect::new(394., 424., 104., 56.), 28., c(0xffffff, 0.16));
    p.accent(state, Rect::new(394., 424., 104., 56.), 28., on);
    p.shadow(Rect::new(400. + 48. * on, 430., 44., 44.), 22., 6., 0.25);
    p.fill(
        Rect::new(400. + 48. * on, 430., 44., 44.),
        22.,
        c(0xffffff, 1.),
    );
}

fn wallpaper(p: &mut Pen, state: &State, assets: &Assets, t: f64) {
    // Two ghost frames suggest the collection behind the current wallpaper.
    p.fill(Rect::new(98., 18., 440., 280.), 16., c(0xffffff, 0.03));
    p.stroke(Rect::new(98., 18., 440., 280.), 16., 1., c(0xffffff, 0.04));
    p.fill(Rect::new(68., 36., 500., 300.), 16., c(0xffffff, 0.048));
    p.stroke(Rect::new(68., 36., 500., 300.), 16., 1., c(0xffffff, 0.064));
    p.shadow(Rect::new(38., 56., 560., 316.), 18., 28., 0.35);
    p.fill(Rect::new(38., 56., 560., 316.), 18., c(0x0b0d10, 1.));
    let q = phase(t, 15.);
    let (base, next, u) = if q < 0.30 {
        (0, 1, 0.)
    } else if q < 0.36 {
        (0, 1, (q - 0.30) / 0.06)
    } else if q < 0.63 {
        (1, 2, 0.)
    } else if q < 0.69 {
        (1, 2, (q - 0.63) / 0.06)
    } else if q < 0.94 {
        (2, 0, 0.)
    } else {
        (2, 0, (q - 0.94) / 0.06)
    };
    let u = smooth(u);
    let stills = stills(state, assets);
    // Hidden stills stay in the frame at near-zero alpha, so their textures
    // are never evicted and decoded again in the middle of the loop.
    for (i, path) in stills.iter().enumerate() {
        if i != base && i != next {
            p.image(path.clone(), Rect::new(46., 64., 544., 300.), 12., 0.004);
        }
    }
    p.image(
        stills[base].clone(),
        Rect::new(46., 64., 544., 300.),
        12.,
        1.,
    );
    p.image(
        stills[next].clone(),
        Rect::new(46., 64., 544., 300.),
        12.,
        u.max(0.004),
    );
    p.fill(Rect::new(298., 372., 40., 22.), 0., c(0x0b0d10, 1.));
    p.fill(Rect::new(228., 390., 180., 10.), 5., c(0x0b0d10, 1.));
    // Source chips light up with the matching still.
    let mut on = [0.; 3];
    on[base] = 1. - u;
    on[next] += u;
    let style = p.style(22., 600, false);
    let labels = ["Apple", "Amazon", "Community"];
    let widths = labels.map(|l| p.width(l, style) + 48.);
    let mut x = 318. - (widths.iter().sum::<f32>() + 24.) / 2.;
    for (i, label) in labels.iter().enumerate() {
        p.fill(Rect::new(x, 444., widths[i], 52.), 26., c(0xffffff, 0.08));
        p.fill(
            Rect::new(x, 444., widths[i], 52.),
            26.,
            c(0xffffff, 0.92 * on[i]),
        );
        let ink = blend(c(0xffffff, 0.72), c(0x15171b, 1.), on[i]);
        p.label_centered(label, x, widths[i], 470., style, ink);
        x += widths[i] + 12.;
    }
}

fn cloud(p: &mut Pen, x: f32, y: f32, k: f32) {
    // Smooth cloud: one capsule and two circles, nothing else.
    let color = c(0xf2f5fa, 1.);
    p.fill(Rect::new(x, y + 70. * k, 300. * k, 80. * k), 40. * k, color);
    p.disc(x + 95. * k, y + 85. * k, 110. * k, color);
    p.disc(x + 180. * k, y + 70. * k, 140. * k, color);
}

fn weather(p: &mut Pen, t: f64) {
    let (cx, cy) = (410., 150.);
    // Three glow rings swell out of the sun one after another and fade.
    for delay in [0., 1.2, 2.4] {
        let p0 = phase(t - delay, 3.6);
        let q = 1. - (1. - p0) * (1. - p0);
        p.disc(cx, cy, 140. * (1. + 1.1 * q), c(0xffe94a, 0.32 * (1. - q)));
    }
    let d = 140. * (1. + 0.04 * (0.5 - 0.5 * (TAU * phase(t, 4.)).cos()));
    p.grad(
        Rect::new(cx - d / 2., cy - d / 2., d, d),
        d / 2.,
        c(0xfff27a, 1.),
        c(0xffc64a, 1.),
    );
    cloud(p, 24. + (20. - 50. * swing(t, 12.)) * 0.62, 40., 0.62);
    cloud(p, 96. - 18. + 44. * swing(t, 9.), 150., 1.);
    p.label_centered(
        "18°",
        0.,
        636.,
        416.,
        p.style(104., 800, true).tracking(-0.02),
        WHITE,
    );
    p.label_centered(
        "Mostly sunny",
        0.,
        636.,
        494.,
        p.style(26., 600, false),
        c(0xffffff, 0.72),
    );
}

/// Point on the cable's centerline, from the device box (0) to the port (1).
fn cable_point(u: f32) -> (f32, f32) {
    let r = 33.;
    let arc = PI * r / 2.;
    let lengths = [62., arc, arc, 44.];
    let mut d = u.clamp(0., 1.) * lengths.iter().sum::<f32>();
    if d <= lengths[0] {
        return (318., 398. - d);
    }
    d -= lengths[0];
    if d <= lengths[1] {
        let a = PI + d / r;
        return (351. + r * a.cos(), 336. + r * a.sin());
    }
    d -= lengths[1];
    if d <= lengths[2] {
        let a = PI / 2. - d / r;
        return (351. + r * a.cos(), 270. + r * a.sin());
    }
    d -= lengths[2];
    (384., 270. - d.min(44.))
}

fn inputs(p: &mut Pen, state: &State, t: f64) {
    let q = phase(t, 6.);
    p.fill(Rect::new(48., 74., 540., 132.), 28., c(0xffffff, 0.06));
    p.stroke(Rect::new(48., 74., 540., 132.), 28., 1., c(0xffffff, 0.08));
    let led3 = keys(
        q,
        &[
            (0., 0.),
            (0.38, 0.),
            (0.42, 1.),
            (0.84, 1.),
            (0.88, 0.),
            (1., 0.),
        ],
        smooth,
    );
    let grey = c(0xffffff, 0.18);
    let green = c(0x4ade80, 1.);
    let leds = [green, green, blend(grey, green, led3), grey];
    let style = p.style(20., 600, false);
    for (i, led) in leds.into_iter().enumerate() {
        let label = format!("HDMI {}", i + 1);
        let w = 18. + p.width(&label, style);
        let x = 54. + 132. * i as f32 + (132. - w) / 2.;
        p.disc(x + 5., 106., 10., led);
        p.label(&label, x + 18., w, 106., style, c(0xffffff, 0.6));
        let px = 74. + 132. * i as f32;
        p.fill(Rect::new(px, 136., 92., 48.), 12., c(0x0b0d10, 1.));
        p.stroke(Rect::new(px, 136., 92., 48.), 12., 2., c(0xffffff, 0.18));
        p.fill(Rect::new(px + 14., 152., 60., 12.), 6., c(0xffffff, 0.14));
    }
    // The port pulses once when the signal arrives.
    let pulse = keys(
        q,
        &[(0., 0.), (0.38, 0.), (0.40, 1.), (0.62, 0.), (1., 0.)],
        linear,
    );
    if pulse > 0. {
        let k = keys(
            q,
            &[(0., 1.), (0.40, 1.), (0.62, 1.35), (1., 1.35)],
            motion::ease_out,
        );
        let (w, h) = (92. * k, 48. * k);
        p.stroke(
            Rect::new(384. - w / 2., 160. - h / 2., w, h),
            12. * k,
            3.,
            c(0x4ade80, pulse),
        );
    }
    // An S-shaped cable from the box up into HDMI 3. Arcs are stamped with
    // round caps of the cable width, so no clipping is needed.
    let cable = c(0x2b2f36, 1.);
    p.fill(Rect::new(308., 336., 20., 62.), 0., cable);
    for j in 0..=24 {
        let a = j as f32 / 24. * PI / 2.;
        p.disc(351. - 33. * a.cos(), 336. - 33. * a.sin(), 20., cable);
        p.disc(351. + 33. * a.sin(), 270. + 33. * a.cos(), 20., cable);
    }
    let lift = keys(
        q,
        &[
            (0., 0.45),
            (0.22, 0.45),
            (0.36, 1.),
            (0.82, 1.),
            (0.94, 0.45),
            (1., 0.45),
        ],
        snappy,
    );
    p.fill(
        Rect::new(374., 270. - 44. * lift, 20., 44. * lift),
        0.,
        cable,
    );
    let dy = 24. * (1. - lift) / 0.55;
    p.fill(Rect::new(356., 168. + dy, 56., 18.), 4., c(0xc9ced6, 1.));
    p.fill(Rect::new(346., 184. + dy, 76., 44.), 12., c(0x3a3f48, 1.));
    p.stroke(
        Rect::new(346., 184. + dy, 76., 44.),
        12.,
        1.,
        c(0xffffff, 0.1),
    );
    // The signal travels from the device to the port.
    let dot = keys(
        q,
        &[
            (0., 0.),
            (0.44, 0.),
            (0.48, 1.),
            (0.74, 1.),
            (0.78, 0.),
            (1., 0.),
        ],
        linear,
    );
    if dot > 0. {
        let (x, y) = cable_point(keys(
            q,
            &[(0., 0.), (0.48, 0.), (0.74, 1.), (1., 1.)],
            smooth,
        ));
        p.accent(state, Rect::new(x - 6., y - 6., 12., 12.), 6., dot);
    }
    p.fill(Rect::new(188., 398., 260., 76.), 20., c(0x22262d, 1.));
    p.stroke(Rect::new(188., 398., 260., 76.), 20., 1., c(0xffffff, 0.1));
    p.label_centered(
        "Apple TV",
        188.,
        260.,
        430.,
        p.style(22., 700, false),
        c(0xffffff, 0.85),
    );
    p.accent(state, Rect::new(284., 454., 68., 6.), 3., 1.);
}

fn smart_home(p: &mut Pen, state: &State, t: f64) {
    let q = phase(t, 8.);
    let on = |a: f32, b: f32| {
        keys(
            q,
            &[(0., 0.), (a, 0.), (b, 1.), (0.82, 1.), (0.90, 0.), (1., 0.)],
            smooth,
        )
    };
    let lit = [
        on(0.06, 0.12),
        on(0.24, 0.30),
        on(0.42, 0.48),
        on(0.60, 0.66),
    ];
    let tint = [
        (0xffe58a, 0.14),
        (0x3fd3ff, 0.12),
        (0xffe94a, 0.10),
        (0x3fd3ff, 0.10),
    ];
    let tiles = [(36., 32.), (330., 32.), (36., 286.), (330., 286.)];
    let names = [
        ("Lamp", "Living room"),
        ("TV plug", "Living room"),
        ("Climate", "Bedroom"),
        ("Blinds", "Bedroom"),
    ];
    let name_style = p.style(24., 700, false);
    let room_style = p.style(20., 600, false);
    for (i, &(x, y)) in tiles.iter().enumerate() {
        p.fill(Rect::new(x, y, 270., 230.), 24., c(0xffffff, 0.06));
        p.fill(
            Rect::new(x, y, 270., 230.),
            24.,
            c(tint[i].0, tint[i].1 * lit[i]),
        );
        p.stroke(Rect::new(x, y, 270., 230.), 24., 1., c(0xffffff, 0.08));
        p.label(names[i].0, x + 28., 220., y + 162., name_style, WHITE);
        p.label(
            names[i].1,
            x + 28.,
            220.,
            y + 194.,
            room_style,
            c(0xffffff, 0.6),
        );
    }
    // Lamp: the bulb lights and its halo breathes.
    let (x, y) = tiles[0];
    let halo = keys(
        q,
        &[
            (0., 0.),
            (0.08, 0.),
            (0.2, 1.),
            (0.4, 0.6),
            (0.6, 1.),
            (0.8, 0.7),
            (0.9, 0.),
            (1., 0.),
        ],
        smooth,
    );
    let k = keys(
        q,
        &[
            (0., 0.8),
            (0.08, 0.8),
            (0.2, 1.),
            (0.4, 1.08),
            (0.6, 1.),
            (0.9, 0.8),
            (1., 0.8),
        ],
        smooth,
    );
    p.disc(x + 64., y + 64., 116. * k, c(0xffe58a, 0.18 * halo));
    p.disc(x + 64., y + 64., 72., c(0xffffff, 0.22));
    p.disc(x + 64., y + 64., 72., c(0xffe58a, lit[0]));
    p.fill(Rect::new(x + 50., y + 98., 28., 16.), 4., c(0xffffff, 0.3));
    // TV plug: a switch.
    let (x, y) = tiles[1];
    let knob = keys(
        q,
        &[
            (0., 0.),
            (0.24, 0.),
            (0.30, 48.),
            (0.82, 48.),
            (0.90, 0.),
            (1., 0.),
        ],
        snappy,
    );
    p.fill(
        Rect::new(x + 28., y + 40., 104., 56.),
        28.,
        c(0xffffff, 0.16),
    );
    p.accent(state, Rect::new(x + 28., y + 40., 104., 56.), 28., lit[1]);
    p.fill(
        Rect::new(x + 34. + knob, y + 46., 44., 44.),
        22.,
        c(0xffffff, 1.),
    );
    // Climate: the dial ring fills with the accent.
    let (x, y) = tiles[2];
    p.stroke(
        Rect::new(x + 28., y + 24., 96., 96.),
        48.,
        10.,
        c(0xffffff, 0.14),
    );
    p.accent_ring(
        state,
        Rect::new(x + 28., y + 24., 96., 96.),
        48.,
        10.,
        lit[2],
    );
    p.label_centered("21°", x + 28., 96., y + 72., p.style(28., 800, true), WHITE);
    // Blinds close over the window.
    let (x, y) = tiles[3];
    let k = keys(
        q,
        &[
            (0., 0.18),
            (0.6, 0.18),
            (0.68, 1.),
            (0.82, 1.),
            (0.9, 0.18),
            (1., 0.18),
        ],
        snappy,
    );
    p.vgrad(
        Rect::new(x + 28., y + 24., 112., 100.),
        12.,
        c(0x3a6a92, 1.),
        c(0x9cc3d8, 1.),
    );
    for i in 0..5 {
        let h = 14. * k;
        p.fill(
            Rect::new(x + 31., y + 27. + 19. * i as f32 * k, 106., h),
            3f32.min(h / 2.),
            c(0xe9edf2, 1.),
        );
    }
    p.stroke(
        Rect::new(x + 28., y + 24., 112., 100.),
        12.,
        3.,
        c(0xffffff, 0.3),
    );
}

fn system(p: &mut Pen, state: &State, assets: &Assets, t: f64) {
    let trace = c(0xffffff, 0.10);
    for y in [144., 208., 272.] {
        p.fill(Rect::new(24., y, 168., 4.), 2., trace);
        p.fill(Rect::new(444., y, 168., 4.), 2., trace);
    }
    for x in [252., 316., 380.] {
        p.fill(Rect::new(x, 12., 4., 72.), 2., trace);
        p.fill(Rect::new(x, 336., 4., 72.), 2., trace);
    }
    let pin = c(0xffffff, 0.24);
    for i in 0..5 {
        let o = 32. * i as f32;
        p.fill(Rect::new(247. + o, 86., 14., 24.), 4., pin);
        p.fill(Rect::new(247. + o, 310., 14., 24.), 4., pin);
        p.fill(Rect::new(194., 139. + o, 24., 14.), 4., pin);
        p.fill(Rect::new(418., 139. + o, 24., 14.), 4., pin);
    }
    // Data pulses run along the traces toward the chip.
    let pulses = [
        (24., 144., 140., 0., 0.),
        (24., 272., 140., 0., 1.4),
        (584., 208., -140., 0., 0.7),
        (316., 12., 0., 56., 2.1),
        (252., 380., 0., -48., 1.),
        (380., 380., 0., -48., 2.4),
    ];
    for (x, y, dx, dy, delay) in pulses {
        let q = phase(t - delay, 3.);
        let alpha = keys(q, &[(0., 0.), (0.15, 1.), (0.85, 1.), (1., 0.)], linear);
        let m = ease_in(q);
        let (w, h) = if dx != 0. { (28., 4.) } else { (4., 28.) };
        p.accent(state, Rect::new(x + dx * m, y + dy * m, w, h), 2., alpha);
    }
    p.fill(Rect::new(218., 110., 200., 200.), 32., c(0x1c1f25, 1.));
    p.stroke(
        Rect::new(218., 110., 200., 200.),
        32.,
        2.,
        c(0xffffff, 0.16),
    );
    let g = 0.5 - 0.5 * (TAU * phase(t, 3.)).cos();
    let k = 148. * (1. + 0.06 * g);
    p.accent(
        state,
        Rect::new(318. - k / 2., 210. - k / 2., k, k),
        30.,
        0.22 * (0.4 + 0.6 * g),
    );
    p.accent(state, Rect::new(252., 144., 128., 128.), 24., 1.);
    p.label_centered(
        "α9",
        252.,
        128.,
        208.,
        p.style(56., 800, true),
        accent_ink(state),
    );
    // Version and root badges share one width.
    let style = p.style(22., 700, false);
    let root = state.root.is_some();
    let version = format!("Luma {}", assets.version.trim());
    let badges = [version.as_str(), if root { "Root" } else { "No root" }];
    for (i, label) in badges.iter().enumerate() {
        let x = 110. + 216. * i as f32;
        p.fill(Rect::new(x, 446., 200., 56.), 28., c(0xffffff, 0.08));
        let w = 24. + p.width(label, style);
        let lx = x + (200. - w) / 2.;
        if i == 0 {
            p.accent(state, Rect::new(lx, 468., 12., 12.), 6., 1.);
        } else if root {
            let a = 1. - 0.55 * (0.5 - 0.5 * (TAU * phase(t, 2.)).cos());
            p.disc(lx + 6., 474., 12., c(0x4ade80, a));
        } else {
            p.disc(lx + 6., 474., 12., c(0xffffff, 0.3));
        }
        p.label(label, lx + 24., w, 474., style, WHITE);
    }
}

fn appearance(p: &mut Pen, t: f64) {
    use crate::typography::HeadingFont;
    let q = phase(t, 8.);
    let k = [
        keys(
            q,
            &[(0., 1.), (0.22, 1.), (0.26, 0.), (0.96, 0.), (1., 1.)],
            smooth,
        ),
        keys(
            q,
            &[
                (0., 0.),
                (0.22, 0.),
                (0.26, 1.),
                (0.47, 1.),
                (0.51, 0.),
                (1., 0.),
            ],
            smooth,
        ),
        keys(
            q,
            &[
                (0., 0.),
                (0.47, 0.),
                (0.51, 1.),
                (0.72, 1.),
                (0.76, 0.),
                (1., 0.),
            ],
            smooth,
        ),
        keys(
            q,
            &[(0., 0.), (0.72, 0.), (0.76, 1.), (0.96, 1.), (1., 0.)],
            smooth,
        ),
    ];
    let over = over_alphas(k);
    let gradient = |name: &str| {
        ACCENT_GRADIENTS
            .iter()
            .find(|g| g.0 == name)
            .map_or((0xa978ff, 0x3fa2ff), |g| (g.1, g.2))
    };
    let shown = ["Citrus", "Sunset", "Mint", "Aurora"].map(gradient);
    // Navigation pill: the Settings tab takes each accent in turn.
    p.fill(Rect::new(90., 56., 456., 76.), 38., c(0xffffff, 0.08));
    let mut ink = [0.; 4];
    for (i, &(a, b)) in shown.iter().enumerate() {
        if over[i] > 0. {
            p.grad(
                Rect::new(396., 62., 144., 64.),
                32.,
                c(a, over[i]),
                c(b, over[i]),
            );
        }
        let tone = crate::view::contrast_text_for_accent(a, b);
        for ch in 0..4 {
            ink[ch] += tone[ch] * k[i];
        }
    }
    let nav = p.style(24., 700, false);
    p.label_centered("Home", 90., 152., 94., nav, c(0xffffff, 0.8));
    p.label_centered("Apps", 242., 152., 94., nav, c(0xffffff, 0.8));
    p.label_centered("Settings", 392., 152., 94., nav, ink);
    // Every gradient swatch; the ring visits the one in use.
    for (i, (_, a, b)) in ACCENT_GRADIENTS.iter().enumerate() {
        p.grad(
            Rect::new(56. + 68. * i as f32, 220., 48., 48.),
            24.,
            c(*a, 1.),
            c(*b, 1.),
        );
    }
    let ring = keys(
        q,
        &[
            (0., 272.),
            (0.20, 272.),
            (0.26, 68.),
            (0.45, 68.),
            (0.51, 136.),
            (0.70, 136.),
            (0.76, 0.),
            (0.95, 0.),
            (1., 272.),
        ],
        snappy,
    );
    p.stroke(
        Rect::new(48. + ring, 212., 64., 64.),
        32.,
        3.,
        c(0xffffff, 1.),
    );
    // Type card: the specimen cycles heading fonts with the accent.
    p.fill(Rect::new(58., 332., 520., 168.), 24., c(0xffffff, 0.06));
    p.stroke(Rect::new(58., 332., 520., 168.), 24., 1., c(0xffffff, 0.08));
    let fonts = [
        (HeadingFont::Nunito, 800, 72.),
        (HeadingFont::Fraunces, 700, 72.),
        (HeadingFont::Bebasneue, 400, 84.),
        (HeadingFont::Montserrat, 700, 72.),
    ];
    let name = p.style(24., 700, false);
    for (i, &(font, weight, size)) in fonts.iter().enumerate() {
        if k[i] <= 0.004 {
            continue;
        }
        let specimen = p.heading(size, weight, font);
        p.label("Aa", 90., 130., 400., specimen, c(0xffffff, 0.96 * k[i]));
        p.label(font.name(), 218., 330., 378., name, c(0xffffff, 0.9 * k[i]));
    }
    p.fill(Rect::new(218., 410., 220., 14.), 7., c(0xffffff, 0.14));
    p.fill(Rect::new(90., 460., 456., 10.), 5., c(0xffffff, 0.1));
    for (i, &(a, b)) in shown.iter().enumerate() {
        if over[i] > 0. {
            p.grad(
                Rect::new(90., 460., 300., 10.),
                5.,
                c(a, over[i]),
                c(b, over[i]),
            );
        }
    }
}

/// Focused card in the portrait row: right to the eighth card and back.
fn stacks_focus(t: f64) -> f32 {
    const MOVE: f32 = 0.32;
    const STEP: f32 = 0.6;
    const SEQ: [f32; 15] = [0., 1., 2., 3., 4., 5., 6., 7., 6., 5., 4., 3., 2., 1., 0.];
    let mut x = t.rem_euclid(10.3) as f32 - 0.8;
    if x < 0. {
        return 0.;
    }
    for w in SEQ.windows(2) {
        if x < MOVE {
            return w[0] + (w[1] - w[0]) * snappy(x / MOVE);
        }
        x -= MOVE;
        let hold = if w[1] == 7. { 0.8 } else { STEP - MOVE };
        if x < hold {
            return w[1];
        }
        x -= hold;
    }
    0.
}

/// Real cached covers from the user's stacks; dark cards until any exist.
fn poster_card(p: &mut Pen, posters: &[&str], n: usize, x: f32, y: f32, w: f32, h: f32) {
    if posters.is_empty() {
        p.fill(Rect::new(x, y, w, h), 10., c(0xffffff, 0.10));
    } else {
        p.image(
            posters[n % posters.len()].to_owned(),
            Rect::new(x, y, w, h),
            10.,
            1.,
        );
    }
}

fn stacks(p: &mut Pen, state: &State, assets: &Assets, t: f64) {
    p.fill(Rect::new(0., 36., 636., 476.), 24., c(0x000000, 0.22));
    p.stroke(Rect::new(0., 36., 636., 476.), 24., 1., c(0xffffff, 0.08));
    let posters: Vec<&str> = state
        .stack_items
        .iter()
        .flatten()
        .filter_map(|item| item.poster.as_deref())
        .take(16)
        .collect();
    let f = stacks_focus(t);
    let slot = f.min(2.);
    let shift = (f - 2.).max(0.) * 116.;
    p.accent_ring(
        state,
        Rect::new(20. + slot * 116., 202., 116., 166.),
        14.,
        3.,
        1.,
    );
    p.clip_x(1., 634.);
    for (i, path) in stills(state, assets).into_iter().enumerate() {
        p.image(
            path,
            Rect::new(28. + 232. * i as f32, 64., 216., 122.),
            10.,
            1.,
        );
    }
    for i in 0..10 {
        let x = 28. + 116. * i as f32 - shift;
        if !(-110. ..=640.).contains(&x) {
            continue;
        }
        let z = 1. + 0.08 * (1. - (f - i as f32).abs()).max(0.);
        let (w, h) = (100. * z, 150. * z);
        poster_card(p, &posters, i, x + 50. - w / 2., 285. - h / 2., w, h);
    }
    for i in 0..6 {
        poster_card(p, &posters, 10 + i, 28. + 116. * i as f32, 384., 100., 100.);
    }
    p.unclip(state);
}
