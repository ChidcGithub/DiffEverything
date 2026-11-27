use crate::model::WaveformDiff;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

const MAX_WINDOWS: usize = 1024;
const MAX_PACKETS: usize = 200_000;
const WINDOW_MS: u32 = 100;

pub struct Waveform {
    pub sample_rate: u32,
    pub channels: usize,
    pub windows: Vec<f32>,
    pub truncated: bool,
}

struct WaveformAccumulator {
    sample_rate: u32,
    channels: usize,
    window_frames: usize,
    frames: usize,
    sum_squares: f64,
    windows: Vec<f32>,
    truncated: bool,
}

impl WaveformAccumulator {
    fn new(sample_rate: u32, channels: usize) -> Self {
        Self {
            sample_rate,
            channels,
            window_frames: (sample_rate / 1000 * WINDOW_MS).max(1) as usize,
            frames: 0,
            sum_squares: 0.0,
            windows: Vec::new(),
            truncated: false,
        }
    }

    fn push_samples(&mut self, samples: &[f32]) {
        if self.truncated || self.channels == 0 {
            return;
        }
        for frame in samples.chunks_exact(self.channels) {
            let energy: f64 = frame.iter().map(|sample| f64::from(*sample).powi(2)).sum();
            self.sum_squares += energy / self.channels as f64;
            self.frames += 1;
            if self.frames >= self.window_frames {
                self.windows
                    .push((self.sum_squares / self.frames as f64).sqrt() as f32);
                self.frames = 0;
                self.sum_squares = 0.0;
                if self.windows.len() >= MAX_WINDOWS {
                    self.truncated = true;
                    return;
                }
            }
        }
    }

    fn finish(mut self) -> Waveform {
        if self.frames > 0 {
            self.windows
                .push((self.sum_squares / self.frames as f64).sqrt() as f32);
        }
        if self.windows.len() > 256 {
            let chunk = self.windows.len().div_ceil(256);
            let mut reduced = Vec::with_capacity(256);
            for group in self.windows.chunks(chunk) {
                reduced.push(group.iter().sum::<f32>() / group.len() as f32);
            }
            self.windows = reduced;
            self.truncated = true;
        }
        Waveform {
            sample_rate: self.sample_rate,
            channels: self.channels,
            windows: self.windows,
            truncated: self.truncated,
        }
    }
}

pub fn audio_waveform(path: &Path) -> Result<Waveform, String> {
    let mut format = open_format(path)?;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .is_some_and(|params| params.audio().is_some())
        })
        .cloned()
        .ok_or_else(|| "media: no audio track".to_string())?;
    let params = track
        .codec_params
        .clone()
        .ok_or_else(|| "media: audio track has no codec parameters".to_string())?;
    let audio_params = params
        .audio()
        .ok_or_else(|| "media: track is not audio".to_string())?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(audio_params, &AudioDecoderOptions::default())
        .map_err(|error| format!("media decoder: {error}"))?;
    let mut accumulator = None;
    let mut packets = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::IoError(_)) => break,
            Err(error) => return Err(format!("media packet: {error}")),
        };
        if packet.track_id != track.id {
            continue;
        }
        packets += 1;
        if packets > MAX_PACKETS {
            break;
        }
        if let Ok(decoded) = decoder.decode(&packet) {
            let accumulator = accumulator.get_or_insert_with(|| {
                WaveformAccumulator::new(decoded.spec().rate(), decoded.spec().channels().count())
            });
            let mut samples = Vec::new();
            decoded.copy_to_vec_interleaved(&mut samples);
            accumulator.push_samples(&samples);
            if accumulator.truncated {
                break;
            }
        }
    }
    accumulator
        .map(WaveformAccumulator::finish)
        .ok_or_else(|| "media: audio track decoded no frames".to_string())
}

pub fn waveform_diff(left: Waveform, right: Waveform) -> WaveformDiff {
    let windows = left.windows.len().min(right.windows.len());
    let mut max_delta = 0.0f32;
    let mut changed_windows = 0usize;
    for index in 0..windows {
        let delta = (left.windows[index] - right.windows[index]).abs();
        max_delta = max_delta.max(delta);
        if delta > 0.01 {
            changed_windows += 1;
        }
    }
    WaveformDiff {
        sample_rate: left.sample_rate,
        channels: left.channels,
        window_ms: WINDOW_MS,
        left: left.windows,
        right: right.windows,
        max_delta,
        changed_windows,
        truncated: left.truncated || right.truncated,
    }
}

fn open_format(path: &Path) -> Result<Box<dyn FormatReader>, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("media open: {error}"))?;
    let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }
    symphonia::default::get_probe()
        .probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|error| format!("media probe: {error}"))
}

pub fn video_value(path: &Path) -> Result<Value, String> {
    let mut format = open_format(path)?;
    let container = format.format_info().short_name.to_string();
    let media_info = format!("{:?}", format.media_info());
    let tracks: Vec<Value> = format
        .tracks()
        .iter()
        .map(|track| {
            let params = track.codec_params.as_ref();
            let (kind, codec, sample_rate, channels, width, height) = match params {
                Some(params) => match params {
                    symphonia::core::codecs::CodecParameters::Audio(audio) => (
                        "audio",
                        format!("{:?}", audio.codec),
                        audio.sample_rate,
                        audio.channels.as_ref().map(|channels| channels.count()),
                        None,
                        None,
                    ),
                    symphonia::core::codecs::CodecParameters::Video(video) => (
                        "video",
                        format!("{:?}", video.codec),
                        None,
                        None,
                        video.width,
                        video.height,
                    ),
                    symphonia::core::codecs::CodecParameters::Subtitle(subtitle) => (
                        "subtitle",
                        format!("{:?}", subtitle.codec),
                        None,
                        None,
                        None,
                        None,
                    ),
                    _ => ("unknown", String::new(), None, None, None, None),
                },
                None => ("unknown", String::new(), None, None, None, None),
            };
            json!({
                "id": track.id,
                "kind": kind,
                "codec": codec,
                "language": track.language,
                "sample_rate": sample_rate,
                "channels": channels,
                "width": width,
                "height": height,
                "num_frames": track.num_frames,
                "start_ts": track.start_ts.get(),
                "duration": format!("{:?}", track.duration),
                "time_base": format!("{:?}", track.time_base),
            })
        })
        .collect();
    let audio_track = format
        .tracks()
        .iter()
        .find(|track| {
            track
                .codec_params
                .as_ref()
                .is_some_and(|params| params.audio().is_some())
        })
        .cloned();
    let mut decoder: Option<Box<dyn AudioDecoder>> = audio_track.as_ref().and_then(|track| {
        track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .and_then(|audio| {
                symphonia::default::get_codecs()
                    .make_audio_decoder(audio, &AudioDecoderOptions::default())
                    .ok()
            })
    });
    let mut waveform: Option<WaveformAccumulator> = None;
    let mut hashes: BTreeMap<u32, Sha256> = BTreeMap::new();
    let mut packet_counts: BTreeMap<u32, u64> = BTreeMap::new();
    let mut packet_bytes: BTreeMap<u32, u64> = BTreeMap::new();
    let mut first_pts: BTreeMap<u32, i64> = BTreeMap::new();
    let mut last_pts: BTreeMap<u32, i64> = BTreeMap::new();
    let mut packet_total = 0usize;
    let mut packet_truncated = false;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::IoError(_)) => break,
            Err(error) => return Err(format!("media packet: {error}")),
        };
        packet_total += 1;
        if packet_total > MAX_PACKETS {
            packet_truncated = true;
            break;
        }
        hashes
            .entry(packet.track_id)
            .or_default()
            .update(&packet.data);
        *packet_counts.entry(packet.track_id).or_default() += 1;
        *packet_bytes.entry(packet.track_id).or_default() += packet.data.len() as u64;
        first_pts.entry(packet.track_id).or_insert(packet.pts.get());
        last_pts.insert(packet.track_id, packet.pts.get());
        if let Some(audio_track) = &audio_track
            && packet.track_id == audio_track.id
            && let Some(decoder) = decoder.as_mut()
            && let Ok(decoded) = decoder.decode(&packet)
        {
            let accumulator = waveform.get_or_insert_with(|| {
                WaveformAccumulator::new(decoded.spec().rate(), decoded.spec().channels().count())
            });
            let mut samples = Vec::new();
            decoded.copy_to_vec_interleaved(&mut samples);
            accumulator.push_samples(&samples);
        }
    }
    let track_stats: Vec<Value> = format
        .tracks()
        .iter()
        .map(|track| {
            json!({
                "id": track.id,
                "packets": packet_counts.get(&track.id).copied().unwrap_or(0),
                "bytes": packet_bytes.get(&track.id).copied().unwrap_or(0),
                "first_pts": first_pts.get(&track.id),
                "last_pts": last_pts.get(&track.id),
                "packet_sha256": hashes.get(&track.id).map(|hash| hex(&hash.clone().finalize())),
            })
        })
        .collect();
    let waveform = waveform.map(|accumulator| {
        let waveform = accumulator.finish();
        json!({
            "sample_rate": waveform.sample_rate,
            "channels": waveform.channels,
            "window_ms": WINDOW_MS,
            "rms": waveform.windows,
            "truncated": waveform.truncated,
        })
    });
    Ok(json!({
        "format": "video",
        "container": container,
        "media_info": media_info,
        "tracks": tracks,
        "track_stats": track_stats,
        "audio_waveform": waveform,
        "packets_truncated": packet_truncated,
    }))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
