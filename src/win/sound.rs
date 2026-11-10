//! Plays the end-of-session chime (generated in memory by `crate::chime`).

use std::sync::OnceLock;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

static CHIME: OnceLock<Vec<u8>> = OnceLock::new();

pub fn play_chime() {
    // The buffer must outlive asynchronous playback, so it lives for the whole run.
    let wav = CHIME.get_or_init(crate::chime::chime_wav);
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}
