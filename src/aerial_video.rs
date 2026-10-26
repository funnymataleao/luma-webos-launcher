//! GStreamer software-decoded RGB -> bounded pipe -> OpenGL texture. The C1 hardware decoder only exposed solid green placeholders on CPU readback.
use serde::Deserialize;
use std::{
    fs::OpenOptions,
    io::Read,
    process::{Child, Command, Stdio},
    sync::mpsc::{Receiver, TrySendError, sync_channel},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 540;
pub const BYTES_PER_FRAME: usize = WIDTH * HEIGHT * 3;
#[derive(Deserialize)]
struct Clip {
    name: String,
    url: String,
}
fn clips() -> Vec<Clip> {
    serde_json::from_str(include_str!("video_catalog.json")).unwrap_or_default()
}
pub struct Player {
    child: Option<Child>,
    reader: Option<JoinHandle<()>>,
    frames: Option<Receiver<Vec<u8>>>,
    active_index: Option<usize>,
    collection: Vec<Clip>,
    retry_after: Instant,
    first_frame: bool,
    frame_count: u64,
}
impl Player {
    pub fn new() -> Self {
        Self {
            child: None,
            reader: None,
            frames: None,
            active_index: None,
            collection: clips(),
            retry_after: Instant::now(),
            first_frame: false,
            frame_count: 0,
        }
    }
    pub fn name(&self, index: usize) -> &str {
        self.collection
            .get(index % self.collection.len().max(1))
            .map_or("Apple Aerials", |clip| clip.name.as_str())
    }
    pub fn len(&self) -> usize {
        self.collection.len()
    }
    pub fn active(&self) -> bool {
        self.child.is_some() && self.first_frame
    }
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
    pub fn take_frame(&mut self) -> Option<Vec<u8>> {
        let mut newest = None;
        if let Some(frames) = &self.frames {
            while let Ok(frame) = frames.try_recv() {
                newest = Some(frame);
            }
        }
        // LG lxvideodec can report a frame buffer full of RGB(0,131,0).
        // Never pass a constant green placeholder through to the GPU.
        if let Some(frame) = newest {
            if !plausible_pixels(&frame) {
                eprintln!("Rejected invalid/non-image RGB video frame");
                return None;
            }
            self.first_frame = true;
            self.frame_count += 1;
            return Some(frame);
        }
        None
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.frames = None;
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        self.active_index = None;
        self.first_frame = false;
    }
    fn start(&mut self, index: usize) -> Result<(), String> {
        if self.collection.is_empty() {
            return Err("No aerials".into());
        }
        self.stop();
        let index = index % self.collection.len();
        let url = &self.collection[index].url;
        if !url.starts_with("https://")
            || url.len() > 600
            || url.contains(['\'', '"', '\n', '\r', '\0'])
        {
            return Err("Invalid URL".into());
        }
        let log = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("/media/developer/com.denisefremov.luma/luma-video.log")
            .ok();
        let mut command = Command::new("/usr/bin/gst-launch-1.0");
        command.args([
            "-q",
            "souphttpsrc",
            &format!("location={url}"),
            "!",
            "qtdemux",
            "name=d",
            "d.video_0",
            "!",
            "queue",
            "!",
            "h264parse",
            "!",
            "avdec_h264",
            "max-threads=2",
            "!",
            "videorate",
            "!",
            "video/x-raw,framerate=15/1",
            "!",
            "videoscale",
            "!",
            "videoconvert",
            "!",
            "video/x-raw,format=RGB,width=960,height=540",
            "!",
            "fdsink",
            "fd=1",
            "sync=true",
        ]);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log.map_or_else(Stdio::null, Stdio::from));
        #[cfg(all(target_os = "linux", feature = "webos"))]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                    Ok(())
                });
            }
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("GStreamer spawn: {e}"))?;
        let mut stdout = child.stdout.take().ok_or("No pipe")?;
        let (sender, receiver) = sync_channel(2);
        let reader = thread::spawn(move || {
            loop {
                let mut bytes = vec![0u8; BYTES_PER_FRAME];
                if stdout.read_exact(&mut bytes).is_err() {
                    break;
                }
                match sender.try_send(bytes) {
                    Ok(()) | Err(TrySendError::Full(_)) => {}
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
        });
        self.child = Some(child);
        self.reader = Some(reader);
        self.frames = Some(receiver);
        self.active_index = Some(index);
        self.retry_after = Instant::now() + Duration::from_secs(5);
        eprintln!("Aerial RGB {}x{} pipeline started", WIDTH, HEIGHT);
        Ok(())
    }
    pub fn tick(
        &mut self,
        wanted: Option<usize>,
        excluded: &std::collections::BTreeSet<String>,
        shuffle: bool,
        seed: u64,
    ) -> Option<usize> {
        let Some(wanted) = wanted else {
            if self.child.is_some() {
                self.stop();
            }
            return None;
        };
        if self.collection.is_empty() {
            return None;
        }
        let wanted = wanted % self.collection.len();
        if self.active_index != Some(wanted) {
            if self.retry_after <= Instant::now()
                && let Err(e) = self.start(wanted)
            {
                eprintln!("Aerial start: {e}");
                self.retry_after = Instant::now() + Duration::from_secs(12);
            }
            return None;
        }
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("Aerial exit: {status}");
                    let has_frames = self.first_frame;
                    self.stop();
                    if !status.success() {
                        self.retry_after = Instant::now() + Duration::from_secs(12);
                        return None;
                    }
                    if has_frames {
                        let next =
                            crate::video_catalog::next_index(wanted, excluded, shuffle, seed)
                                .unwrap_or(wanted);
                        if let Err(e) = self.start(next) {
                            eprintln!("Aerial next: {e}");
                        }
                        return Some(next);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Aerial poll: {e}");
                    self.stop();
                    self.retry_after = Instant::now() + Duration::from_secs(12);
                }
            }
        }
        None
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn playlist() {
        assert!(super::clips().len() > 100);
    }
    #[test]
    fn dimensions() {
        assert_eq!(super::BYTES_PER_FRAME, 1555200);
    }
}

// Down-sampled variance guard: prevent the LG hardware-decoder green placeholder
// and uninitialised pixels from being displayed. Dark footage still has variance.
fn plausible_pixels(bytes: &[u8]) -> bool {
    if bytes.len() < 96 || !bytes.len().is_multiple_of(3) {
        return false;
    }
    let count = bytes.len() / 3;
    let stride = (count / 300).max(1);
    let mut min = [255u8; 3];
    let mut max = [0u8; 3];
    for i in (0..count).step_by(stride) {
        let o = i * 3;
        for c in 0..3 {
            min[c] = min[c].min(bytes[o + c]);
            max[c] = max[c].max(bytes[o + c]);
        }
    }
    (0..3).any(|c| u16::from(max[c]) - u16::from(min[c]) > 12)
}
#[cfg(test)]
mod decoder_tests {
    #[test]
    fn reject_solid_green_hardware_placeholder() {
        let solid = [0_u8, 131, 0].repeat(90);
        assert!(!super::plausible_pixels(&solid));
    }
    #[test]
    fn accept_real_video_variation() {
        let colorful = (0..300)
            .flat_map(|i| [i as u8, (i * 13) as u8, 255 - (i % 256) as u8])
            .collect::<Vec<_>>();
        assert!(super::plausible_pixels(&colorful));
    }
}
