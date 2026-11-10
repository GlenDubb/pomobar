//! A short, soft chime, synthesised at runtime as a WAV file in memory.
//! Generating it here means the app ships no audio files.

use std::f32::consts::PI;

const RATE: u32 = 22_050;

/// A rising four-note bell arpeggio (E5, G#5, B5, E6), about two seconds long.
pub fn chime_wav() -> Vec<u8> {
    let notes: [(f32, f32); 4] = [(0.00, 659.25), (0.16, 830.61), (0.32, 987.77), (0.52, 1318.51)];
    let len = (RATE as f32 * 2.0) as usize;
    let mut buf = vec![0f32; len];
    for &(start, freq) in &notes {
        let first = (start * RATE as f32) as usize;
        for (i, sample) in buf.iter_mut().enumerate().skip(first) {
            let t = (i - first) as f32 / RATE as f32;
            let envelope = (t / 0.006).min(1.0) * (-t * 3.2).exp();
            let w = 2.0 * PI * freq * t;
            let tone = w.sin()
                + 0.28 * (2.0 * w).sin() * (-t * 2.5).exp()
                + 0.08 * (3.01 * w).sin() * (-t * 4.0).exp();
            *sample += tone * envelope;
        }
    }
    let peak = buf.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-6);
    let gain = 0.55 / peak;
    let fade_len = 0.05 * RATE as f32;

    let data_len = (len * 2) as u32;
    let mut out = Vec::with_capacity(44 + len * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for (i, v) in buf.iter().enumerate() {
        let fade = ((len - i) as f32 / fade_len).min(1.0);
        let s = (v * gain * fade * 32767.0).clamp(-32768.0, 32767.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_a_valid_pcm_wav() {
        let w = chime_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        let riff_len = u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize;
        assert_eq!(riff_len + 8, w.len());
        let data_len = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
        assert_eq!(data_len + 44, w.len());
        let samples: Vec<i16> = w[44..]
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak > 10_000 && peak < 20_000, "peak {peak}");
        assert_eq!(*samples.last().unwrap(), 0, "fades out to silence");
    }
}
