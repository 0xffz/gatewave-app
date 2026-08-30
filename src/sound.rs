//! The "code received" chime, played through whatever the OS already ships.
//!
//! No audio stack is linked in: a system sound is handed to a platform player on a background
//! thread, so a missing player or sound only costs one stderr line and never blocks a frame.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// Candidate `(program, args)` players, tried in order until one starts.
#[cfg(target_os = "macos")]
const PLAYERS: &[(&str, &[&str])] = &[("afplay", &["/System/Library/Sounds/Glass.aiff"])];

#[cfg(target_os = "linux")]
const PLAYERS: &[(&str, &[&str])] = &[
    ("canberra-gtk-play", &["-i", "complete"]),
    (
        "paplay",
        &["/usr/share/sounds/freedesktop/stereo/complete.oga"],
    ),
    ("aplay", &["-q", "/usr/share/sounds/alsa/Front_Center.wav"]),
];

#[cfg(target_os = "windows")]
const PLAYERS: &[(&str, &[&str])] = &[(
    "powershell",
    &[
        "-NoProfile",
        "-Command",
        "(New-Object Media.SoundPlayer 'C:\\Windows\\Media\\notify.wav').PlaySync()",
    ],
)];

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const PLAYERS: &[(&str, &[&str])] = &[];

static WARNED: AtomicBool = AtomicBool::new(false);

/// Plays the chime without blocking. Reports a missing player once per run.
pub fn chime() {
    thread::spawn(|| {
        for (program, args) in PLAYERS {
            let started = Command::new(program)
                .args(*args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if let Ok(status) = started
                && status.success()
            {
                return;
            }
        }
        if !WARNED.swap(true, Ordering::Relaxed) {
            eprintln!("gatewave: no system sound player found; the chime is off");
        }
    });
}
