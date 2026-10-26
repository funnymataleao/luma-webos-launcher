//! Quiet Apple TV-style navigation clicks, synthesized at start-up.
//! No audio assets: two short damped tones are rendered once into PCM.

use sdl2::audio::{AudioQueue, AudioSpecDesired};

const RATE: i32 = 48_000;

pub struct UiSound {
    queue: AudioQueue<i16>,
    tick: Vec<i16>,
    select: Vec<i16>,
}

/// A soft "tock": a high partial for the click, a low one for body,
/// both with a 1 ms attack and fast exponential decay.
fn tone(high: f32, low: f32, decay_ms: f32, gain: f32) -> Vec<i16> {
    let length = (RATE as f32 * decay_ms * 5. / 1000.) as usize;
    (0..length)
        .map(|n| {
            let t = n as f32 / RATE as f32;
            let attack = (t / 0.001).min(1.);
            let body = (-t / (decay_ms / 1000.)).exp();
            let click = (-t / (decay_ms / 2500.)).exp();
            let wave = 0.6 * (std::f32::consts::TAU * high * t).sin() * click
                + 0.4 * (std::f32::consts::TAU * low * t).sin() * body;
            (wave * attack * gain * i16::MAX as f32) as i16
        })
        .collect()
}

impl UiSound {
    pub fn new(sdl: &sdl2::Sdl) -> Option<Self> {
        let audio = sdl.audio().ok()?;
        let spec = AudioSpecDesired {
            freq: Some(RATE),
            channels: Some(1),
            samples: Some(512),
        };
        let queue = audio.open_queue::<i16, _>(None, &spec).ok()?;
        queue.resume();
        // The subsystem must outlive the device; it is reference counted by SDL.
        std::mem::forget(audio);
        Some(Self {
            queue,
            tick: tone(2100., 420., 9., 0.10),
            select: tone(1500., 300., 14., 0.13),
        })
    }

    fn play(&self, samples: &[i16]) {
        // Rapid moves restart the click instead of stacking a backlog.
        self.queue.clear();
        let _ = self.queue.queue_audio(samples);
    }

    pub fn tick(&self) {
        self.play(&self.tick);
    }

    pub fn select(&self) {
        self.play(&self.select);
    }
}
