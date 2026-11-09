//! Root helper started by Luma (`luma --pad-watch`). A gamepad plugged into
//! the TV goes straight to Moonlight, so Luma cannot see its buttons. This
//! process reads them from evdev, and a double Start+Select while Moonlight
//! streams leaves a flag and brings Luma back, which closes the game.
use std::collections::HashSet;
use std::io::Read;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const FLAG: &str = "/media/developer/com.denisefremov.luma/moonlight-quit";
pub const PIDFILE: &str = "/tmp/luma-pad-watch.pid";
const MOONLIGHT: &str = "com.limelight.webos";
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const EV_KEY: u16 = 1;
const DOUBLE: Duration = Duration::from_millis(900);

/// Two Start+Select chords within `DOUBLE`. A chord counts when the second
/// of the two buttons goes down while the first is held.
#[derive(Default)]
pub struct Chord {
    start: bool,
    select: bool,
    last: Option<Instant>,
}

impl Chord {
    pub fn key(&mut self, code: u16, down: bool, now: Instant) -> bool {
        let before = self.start && self.select;
        match code {
            BTN_START => self.start = down,
            BTN_SELECT => self.select = down,
            _ => return false,
        }
        if before || !(self.start && self.select) {
            return false;
        }
        if self.last.is_some_and(|t| now.duration_since(t) <= DOUBLE) {
            self.last = None;
            return true;
        }
        self.last = Some(now);
        false
    }
}

// struct input_event: timeval, then type u16, code u16, value i32.
const EVENT: usize = if cfg!(target_pointer_width = "64") {
    24
} else {
    16
};

fn listen(path: String, sender: mpsc::Sender<(u16, bool)>) {
    let Ok(mut device) = std::fs::File::open(&path) else {
        return;
    };
    let mut buf = [0u8; EVENT];
    while device.read_exact(&mut buf).is_ok() {
        let at = EVENT - 8;
        let kind = u16::from_ne_bytes([buf[at], buf[at + 1]]);
        let code = u16::from_ne_bytes([buf[at + 2], buf[at + 3]]);
        let value = i32::from_ne_bytes([buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]]);
        // value 2 is autorepeat: neither a press nor a release.
        if kind == EV_KEY
            && (code == BTN_START || code == BTN_SELECT)
            && value != 2
            && sender.send((code, value == 1)).is_err()
        {
            return;
        }
    }
}

fn luna(uri: &str, payload: &str) -> String {
    std::process::Command::new("/usr/bin/luna-send")
        .args(["-n", "1", "-w", "3000", uri, payload])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn moonlight_in_front() -> bool {
    luna(
        "luna://com.webos.applicationManager/getForegroundAppInfo",
        "{}",
    )
    .replace(' ', "")
    .contains(&format!("\"appId\":\"{MOONLIGHT}\""))
}

/// Luma starts this helper through Homebrew Channel's `exec` service, so it inherits that
/// service's luna bus sockets and working directory. The service exits after a while, but as
/// long as this long-lived helper holds those sockets the bus keeps the service registered and
/// routes every later call to a process that no longer exists: app icons, the app list and the
/// root features then wait until they time out. Keep only stdout/stderr (the log redirect).
#[cfg(all(feature = "webos", unix))]
fn detach_from_service() {
    let inherited: Vec<i32> = std::fs::read_dir("/proc/self/fd")
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
                .filter(|fd| *fd > 2)
                .collect()
        })
        .unwrap_or_default();
    // SAFETY: closes inherited descriptors before this process opens any of its own;
    // a descriptor that was only the read_dir handle is already closed and fails harmlessly.
    unsafe {
        for fd in inherited {
            libc::close(fd);
        }
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY);
        if null > 0 {
            libc::dup2(null, 0);
            libc::close(null);
        }
        libc::setsid();
        libc::chdir(c"/".as_ptr());
    }
}

pub fn run() -> ! {
    #[cfg(all(feature = "webos", unix))]
    detach_from_service();
    let _ = std::fs::write(PIDFILE, std::process::id().to_string());
    eprintln!("Luma pad watch started");
    let (sender, receiver) = mpsc::channel();
    let mut watched: HashSet<String> = HashSet::new();
    let mut chord = Chord::default();
    let mut scanned = Instant::now() - Duration::from_secs(10);
    loop {
        // Gamepads come and go (Bluetooth, USB): rescan every few seconds.
        if scanned.elapsed() >= Duration::from_secs(3) {
            scanned = Instant::now();
            if let Ok(entries) = std::fs::read_dir("/dev/input") {
                for entry in entries.flatten() {
                    let path = entry.path().to_string_lossy().into_owned();
                    if path.contains("/event") && watched.insert(path.clone()) {
                        let sender = sender.clone();
                        std::thread::spawn(move || listen(path, sender));
                    }
                }
            }
            // A device that went away can come back under the same name.
            watched.retain(|path| std::path::Path::new(path).exists());
        }
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok((code, down)) => {
                if chord.key(code, down, Instant::now()) && moonlight_in_front() {
                    eprintln!("Luma pad watch: Start+Select twice, closing the game");
                    let _ = std::fs::write(FLAG, b"1");
                    luna(
                        "luna://com.webos.service.applicationManager/launch",
                        "{\"id\":\"com.denisefremov.luma\"}",
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_start_select_fires_once() {
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        let mut c = Chord::default();
        assert!(!c.key(BTN_START, true, ms(0)));
        assert!(!c.key(BTN_SELECT, true, ms(30)));
        assert!(!c.key(BTN_START, false, ms(120)));
        assert!(!c.key(BTN_SELECT, false, ms(130)));
        assert!(!c.key(BTN_SELECT, true, ms(300)));
        assert!(c.key(BTN_START, true, ms(320)));
        // Holding and releasing does not count again.
        assert!(!c.key(BTN_START, false, ms(400)));
        assert!(!c.key(BTN_SELECT, false, ms(410)));
    }

    #[test]
    fn single_or_slow_chords_do_not_fire() {
        let t = Instant::now();
        let ms = |n| t + Duration::from_millis(n);
        let mut c = Chord::default();
        assert!(!c.key(BTN_START, true, ms(0)));
        assert!(!c.key(BTN_START, false, ms(100)));
        assert!(!c.key(BTN_START, true, ms(200)));
        assert!(!c.key(BTN_SELECT, true, ms(220)));
        assert!(!c.key(BTN_SELECT, false, ms(300)));
        assert!(!c.key(BTN_START, false, ms(300)));
        assert!(!c.key(BTN_START, true, ms(1500)));
        assert!(!c.key(BTN_SELECT, true, ms(1510)));
    }
}
