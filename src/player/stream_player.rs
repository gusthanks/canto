//! Local file and HTTP stream audio decoder routed through Canto's
//! shared [`ProcessorChain`] (`Eq`, `Mono`, `Pan`, `Pitch`, `Mix`, `Limiter`,
//! `LevelMeter`) and librespot's PulseAudio/ALSA sink.

use std::convert::TryInto;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};

use canto_audio_engine::{
    CaptureSink, EqController, EqProcessor, LevelMeterController, LevelMeterProcessor,
    LimiterProcessor, MixController, MixProcessor, MonoController, MonoProcessor, PanController,
    PanProcessor, PitchController, PitchProcessor, ProcessorChain,
};
use librespot::playback::audio_backend;
use librespot::playback::audio_backend::Sink;
use librespot::playback::config::AudioFormat;
use librespot::playback::convert::Converter;
use librespot::playback::decoder::AudioPacket;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use super::player::{AudioBackend, VolumeCurveType};
use super::AppPlayerDelegate;

const TARGET_SAMPLE_RATE: u32 = 44_100;
const CHUNK_FRAMES: usize = 2048;

#[derive(Clone)]
struct StreamControl {
    generation: Arc<AtomicU64>,
    paused: Arc<AtomicBool>,
    seek_ms: Arc<AtomicI64>,
    volume_bits: Arc<AtomicU64>,
    pause_gate: Arc<(Mutex<()>, Condvar)>,
}

impl StreamControl {
    fn new(initial_volume: f64) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            paused: Arc::new(AtomicBool::new(false)),
            seek_ms: Arc::new(AtomicI64::new(-1)),
            volume_bits: Arc::new(AtomicU64::new(initial_volume.clamp(0.0, 1.0).to_bits())),
            pause_gate: Arc::new((Mutex::new(()), Condvar::new())),
        }
    }

    fn wake(&self) {
        let (_guard, cv) = &*self.pause_gate;
        cv.notify_all();
    }

    fn current_volume(&self) -> f64 {
        f64::from_bits(self.volume_bits.load(Ordering::Acquire)).clamp(0.0, 1.0)
    }
}

struct StreamJob {
    gen: u64,
    url: String,
    resume: bool,
    backend: AudioBackend,
    audio_format: AudioFormat,
    volume_curve: VolumeCurveType,
}

#[derive(Clone)]
struct SharedEngine {
    eq: EqController,
    mono: MonoController,
    pan: PanController,
    pitch: PitchController,
    mix: MixController,
    level_meter: LevelMeterController,
}

pub(crate) struct StreamPlayer {
    control: StreamControl,
    job_tx: mpsc::Sender<StreamJob>,
    delegate: AppPlayerDelegate,
}

impl StreamPlayer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        initial_volume: f64,
        eq: EqController,
        mono: MonoController,
        pan: PanController,
        pitch: PitchController,
        mix: MixController,
        level_meter: LevelMeterController,
        delegate: AppPlayerDelegate,
    ) -> Self {
        let control = StreamControl::new(initial_volume);
        let engine = SharedEngine {
            eq,
            mono,
            pan,
            pitch,
            mix,
            level_meter,
        };
        let (job_tx, job_rx) = mpsc::channel::<StreamJob>();

        let worker_control = control.clone();
        let worker_delegate = delegate.clone();
        let http_client = reqwest::Client::new();
        std::thread::Builder::new()
            .name("canto-stream-player".to_string())
            .spawn(move || {
                stream_worker_loop(job_rx, worker_control, engine, worker_delegate, http_client);
            })
            .expect("spawn stream player thread");

        Self {
            control,
            job_tx,
            delegate,
        }
    }

    pub(crate) fn load(
        &self,
        url: String,
        resume: bool,
        backend: AudioBackend,
        audio_format: AudioFormat,
        volume_curve: VolumeCurveType,
    ) {
        let gen = self.control.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.control.seek_ms.store(-1, Ordering::Release);
        self.control.paused.store(!resume, Ordering::Release);
        self.control.wake();
        let _ = self.job_tx.send(StreamJob {
            gen,
            url,
            resume,
            backend,
            audio_format,
            volume_curve,
        });
    }

    pub(crate) fn play(&self) {
        self.control.paused.store(false, Ordering::Release);
        self.control.wake();
    }

    pub(crate) fn pause(&self) {
        self.control.paused.store(true, Ordering::Release);
        self.control.wake();
    }

    pub(crate) fn stop(&self) {
        self.control.generation.fetch_add(1, Ordering::AcqRel);
        self.control.paused.store(false, Ordering::Release);
        self.control.seek_ms.store(-1, Ordering::Release);
        self.control.wake();
    }

    pub(crate) fn seek(&self, position_ms: u32) {
        self.control
            .seek_ms
            .store(i64::from(position_ms), Ordering::Release);
        self.control.wake();
        self.delegate.notify_playback_state(position_ms);
    }

    pub(crate) fn set_volume(&self, volume: f64) {
        self.control
            .volume_bits
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Release);
    }
}

fn stream_worker_loop(
    job_rx: mpsc::Receiver<StreamJob>,
    control: StreamControl,
    engine: SharedEngine,
    delegate: AppPlayerDelegate,
    http_client: reqwest::Client,
) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok();

    while let Ok(mut job) = job_rx.recv() {
        // Coalesce bursty rapid track skips so only the latest job runs.
        while let Ok(newer) = job_rx.try_recv() {
            job = newer;
        }
        if control.generation.load(Ordering::Acquire) != job.gen {
            continue;
        }

        if let Err(err) = play_single_job(
            &job,
            &control,
            &engine,
            &delegate,
            &http_client,
            rt.as_ref(),
        ) {
            warn!("StreamPlayer playback failed for {}: {err}", job.url);
            if control.generation.load(Ordering::Acquire) == job.gen {
                delegate.end_of_track_reached();
            }
        }
    }
}

enum OpenedMedia {
    File(File, Option<String>),
    Memory(Vec<u8>, Option<String>),
}

fn open_media_source(
    url: &str,
    http_client: &reqwest::Client,
    rt: Option<&tokio::runtime::Runtime>,
) -> Result<OpenedMedia, String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let runtime = rt.ok_or_else(|| "tokio runtime unavailable for HTTP stream".to_string())?;
        let client = http_client.clone();
        let target = url.to_string();
        let bytes = runtime.block_on(async move {
            let resp = client
                .get(&target)
                .send()
                .await
                .map_err(|e| format!("HTTP GET failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("HTTP status {}", resp.status()));
            }
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| format!("HTTP body error: {e}"))
        })?;
        let ext = url
            .split('?')
            .next()
            .and_then(|p| Path::new(p).extension())
            .and_then(|e| e.to_str())
            .map(str::to_string);
        return Ok(OpenedMedia::Memory(bytes, ext));
    }

    let path = if let Some(rest) = url.strip_prefix("file://") {
        rest.to_string()
    } else {
        url.to_string()
    };
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_string);
    let file = File::open(&path).map_err(|e| format!("open {path}: {e}"))?;
    Ok(OpenedMedia::File(file, ext))
}

fn build_capture_sink(
    backend: &AudioBackend,
    audio_format: AudioFormat,
    engine: &SharedEngine,
) -> Result<Box<dyn Sink>, String> {
    let raw_sink: Box<dyn Sink> = match backend {
        AudioBackend::PulseAudio => {
            let find_fn = audio_backend::find(Some("pulseaudio".to_string()))
                .ok_or_else(|| "pulseaudio backend missing".to_string())?;
            find_fn(None, audio_format)
        }
        AudioBackend::Alsa(dev) => {
            let find_fn = audio_backend::find(Some("alsa".to_string()))
                .ok_or_else(|| "alsa backend missing".to_string())?;
            find_fn(Some(dev.clone()), audio_format)
        }
    };

    let chain = ProcessorChain::new()
        .with(Box::new(EqProcessor::new(engine.eq.clone())))
        .with(Box::new(MonoProcessor::new(engine.mono.clone())))
        .with(Box::new(PanProcessor::new(engine.pan.clone())))
        .with(Box::new(PitchProcessor::new(engine.pitch.clone())))
        .with(Box::new(MixProcessor::new(engine.mix.clone())))
        .with(Box::new(LimiterProcessor::new()))
        .with(Box::new(LevelMeterProcessor::new(
            engine.level_meter.clone(),
        )));

    Ok(CaptureSink::wrap(raw_sink, chain))
}

fn play_single_job(
    job: &StreamJob,
    control: &StreamControl,
    engine: &SharedEngine,
    delegate: &AppPlayerDelegate,
    http_client: &reqwest::Client,
    rt: Option<&tokio::runtime::Runtime>,
) -> Result<(), String> {
    let mut media = open_media_source(&job.url, http_client, rt)?;
    if control.generation.load(Ordering::Acquire) != job.gen {
        return Ok(());
    }

    let is_wav = match &mut media {
        OpenedMedia::File(f, _) => {
            let mut hdr = [0u8; 12];
            let ok =
                f.read_exact(&mut hdr).is_ok() && &hdr[0..4] == b"RIFF" && &hdr[8..12] == b"WAVE";
            let _ = f.seek(SeekFrom::Start(0));
            ok
        }
        OpenedMedia::Memory(bytes, _) => {
            bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
        }
    };

    let mut sink = build_capture_sink(&job.backend, job.audio_format, engine)?;
    let mut converter = Converter::new(None);
    sink.start().map_err(|e| format!("sink start: {e}"))?;

    if !job.resume {
        wait_if_paused(control, job.gen);
        if control.generation.load(Ordering::Acquire) != job.gen {
            let _ = sink.stop();
            return Ok(());
        }
    }

    delegate.notify_playback_state(0);

    let finished = if is_wav {
        match media {
            OpenedMedia::File(f, _) => stream_wav(
                f,
                job.gen,
                job.volume_curve,
                control,
                delegate,
                &mut *sink,
                &mut converter,
            )?,
            OpenedMedia::Memory(bytes, _) => stream_wav(
                Cursor::new(bytes),
                job.gen,
                job.volume_curve,
                control,
                delegate,
                &mut *sink,
                &mut converter,
            )?,
        }
    } else {
        let (source, ext): (Box<dyn MediaSource>, Option<String>) = match media {
            OpenedMedia::File(f, ext) => (Box::new(f), ext),
            OpenedMedia::Memory(bytes, ext) => (Box::new(Cursor::new(bytes)), ext),
        };
        stream_symphonia(
            source,
            ext.as_deref(),
            job.gen,
            job.volume_curve,
            control,
            delegate,
            &mut *sink,
            &mut converter,
        )?
    };

    let _ = sink.stop();
    if finished && control.generation.load(Ordering::Acquire) == job.gen {
        delegate.end_of_track_reached();
    }
    Ok(())
}

fn wait_if_paused(control: &StreamControl, gen: u64) {
    let (lock, cv) = &*control.pause_gate;
    let mut guard = lock.lock().unwrap();
    while control.paused.load(Ordering::Acquire)
        && control.generation.load(Ordering::Acquire) == gen
    {
        guard = cv.wait(guard).unwrap();
    }
}

fn volume_gain(volume: f64, curve: VolumeCurveType) -> f64 {
    let v = volume.clamp(0.0, 1.0);
    match curve {
        VolumeCurveType::Linear => v,
        VolumeCurveType::Cubic => v * v * v,
        VolumeCurveType::Log => {
            if v <= 0.0001 {
                0.0
            } else {
                // Smooth 60 dB logarithmic taper matching hardware mixers.
                10.0_f64.powf((v - 1.0) * 3.0)
            }
        }
    }
}

fn write_stereo_frames(
    samples: Vec<f64>,
    volume_curve: VolumeCurveType,
    control: &StreamControl,
    sink: &mut dyn Sink,
    converter: &mut Converter,
) -> Result<(), String> {
    let gain = volume_gain(control.current_volume(), volume_curve);
    let mut out = samples;
    if (gain - 1.0).abs() > 1e-6 {
        for s in &mut out {
            *s *= gain;
        }
    }
    sink.write(AudioPacket::Samples(out), converter)
        .map_err(|e| format!("sink write: {e}"))
}

/// Convert interleaved PCM of `src_channels` at `src_rate` into interleaved
/// stereo at `TARGET_SAMPLE_RATE` (`44_100 Hz`) via linear interpolation.
pub fn resample_interleaved_to_stereo_44100(
    input: &[f64],
    src_channels: usize,
    src_rate: u32,
) -> Vec<f64> {
    if input.is_empty() || src_channels == 0 || src_rate == 0 {
        return Vec::new();
    }
    let src_frames = input.len() / src_channels;
    if src_frames == 0 {
        return Vec::new();
    }

    let stereo_in: Vec<f64> = if src_channels == 2 {
        input[..src_frames * 2].to_vec()
    } else {
        let mut s = Vec::with_capacity(src_frames * 2);
        for f in 0..src_frames {
            let base = f * src_channels;
            let l = input[base];
            let r = if src_channels > 1 { input[base + 1] } else { l };
            s.push(l);
            s.push(r);
        }
        s
    };

    if src_rate == TARGET_SAMPLE_RATE {
        return stereo_in;
    }

    let ratio = src_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let out_frames = ((src_frames as f64 / ratio).round() as usize).max(1);
    let mut out = Vec::with_capacity(out_frames * 2);
    for out_idx in 0..out_frames {
        let src_pos = (out_idx as f64) * ratio;
        let idx0 = (src_pos.floor() as usize).min(src_frames - 1);
        let idx1 = (idx0 + 1).min(src_frames - 1);
        let frac = src_pos - idx0 as f64;
        let l = stereo_in[idx0 * 2] * (1.0 - frac) + stereo_in[idx1 * 2] * frac;
        let r = stereo_in[idx0 * 2 + 1] * (1.0 - frac) + stereo_in[idx1 * 2 + 1] * frac;
        out.push(l);
        out.push(r);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn stream_symphonia(
    source: Box<dyn MediaSource>,
    ext: Option<&str>,
    gen: u64,
    volume_curve: VolumeCurveType,
    control: &StreamControl,
    delegate: &AppPlayerDelegate,
    sink: &mut dyn Sink,
    converter: &mut Converter,
) -> Result<bool, String> {
    let mss = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = ext {
        hint.with_extension(ext);
    }

    let mut probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("probe: {e}"))?;

    let track = probed
        .format
        .default_track()
        .ok_or_else(|| "no playable audio track found".to_string())?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let src_rate = codec_params.sample_rate.unwrap_or(TARGET_SAMPLE_RATE);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| format!("decoder: {e}"))?;

    let mut sample_buf: Option<SampleBuffer<f64>> = None;
    let mut emitted_ms: u64 = 0;
    let mut last_reported_sec: u64 = 0;

    loop {
        if control.generation.load(Ordering::Acquire) != gen {
            return Ok(false);
        }
        wait_if_paused(control, gen);
        if control.generation.load(Ordering::Acquire) != gen {
            return Ok(false);
        }

        let pending_seek = control.seek_ms.swap(-1, Ordering::AcqRel);
        if pending_seek >= 0 {
            let target_ms = pending_seek as u64;
            let time = Time::new(target_ms / 1000, ((target_ms % 1000) as f64) / 1000.0);
            if probed
                .format
                .seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time,
                        track_id: Some(track_id),
                    },
                )
                .is_ok()
            {
                decoder.reset();
                emitted_ms = target_ms;
                last_reported_sec = emitted_ms / 1000;
                delegate.notify_playback_state(pending_seek as u32);
            }
        }

        let packet = match probed.format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return Ok(true);
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(format!("read packet: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let frames = decoded.frames();
                if frames == 0 {
                    continue;
                }
                let buf = sample_buf.get_or_insert_with(|| {
                    SampleBuffer::<f64>::new(decoded.capacity() as u64, spec)
                });
                buf.copy_interleaved_ref(decoded);
                let rate = if spec.rate > 0 { spec.rate } else { src_rate };
                let ch = spec.channels.count().max(1);
                let stereo = resample_interleaved_to_stereo_44100(buf.samples(), ch, rate);
                if !stereo.is_empty() {
                    let frame_count = (stereo.len() / 2) as u64;
                    write_stereo_frames(stereo, volume_curve, control, sink, converter)?;
                    emitted_ms += (frame_count * 1000) / u64::from(TARGET_SAMPLE_RATE);
                    let cur_sec = emitted_ms / 1000;
                    if cur_sec > last_reported_sec {
                        last_reported_sec = cur_sec;
                        delegate.notify_playback_state(emitted_ms as u32);
                    }
                }
            }
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => return Ok(true),
            Err(e) => return Err(format!("decode: {e}")),
        }
    }
}

struct WavSpec {
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
    audio_format: u16,
    data_offset: u64,
    data_size: u32,
}

fn parse_wav_spec<R: Read + Seek>(mut r: R) -> Result<WavSpec, String> {
    let mut hdr = [0u8; 12];
    r.read_exact(&mut hdr)
        .map_err(|e| format!("wav header: {e}"))?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE stream".to_string());
    }

    let mut channels = 0u16;
    let mut sample_rate = 0u32;
    let mut bits_per_sample = 0u16;
    let mut audio_format = 1u16;
    let mut chunk_hdr = [0u8; 8];
    while r.read_exact(&mut chunk_hdr).is_ok() {
        let size = u32::from_le_bytes(chunk_hdr[4..8].try_into().unwrap());
        if &chunk_hdr[0..4] == b"fmt " {
            let take = size.min(64) as usize;
            let mut fmt = vec![0u8; take];
            r.read_exact(&mut fmt)
                .map_err(|e| format!("wav fmt: {e}"))?;
            if take < 16 {
                return Err("short wav fmt chunk".to_string());
            }
            audio_format = u16::from_le_bytes(fmt[0..2].try_into().unwrap());
            channels = u16::from_le_bytes(fmt[2..4].try_into().unwrap());
            sample_rate = u32::from_le_bytes(fmt[4..8].try_into().unwrap());
            bits_per_sample = u16::from_le_bytes(fmt[14..16].try_into().unwrap());
            if size as usize > take {
                r.seek(SeekFrom::Current((size as usize - take) as i64))
                    .map_err(|e| format!("wav fmt skip: {e}"))?;
            }
        } else if &chunk_hdr[0..4] == b"data" {
            let data_offset = r.stream_position().map_err(|e| format!("wav pos: {e}"))?;
            return Ok(WavSpec {
                channels,
                sample_rate,
                bits_per_sample,
                audio_format,
                data_offset,
                data_size: size,
            });
        } else {
            let skip = i64::from(size) + i64::from(size % 2);
            r.seek(SeekFrom::Current(skip))
                .map_err(|e| format!("wav skip: {e}"))?;
        }
    }
    Err("wav missing data chunk".to_string())
}

fn stream_wav<R: Read + Seek>(
    mut reader: R,
    gen: u64,
    volume_curve: VolumeCurveType,
    control: &StreamControl,
    delegate: &AppPlayerDelegate,
    sink: &mut dyn Sink,
    converter: &mut Converter,
) -> Result<bool, String> {
    let spec = parse_wav_spec(&mut reader)?;
    let bytes_per_sample = (spec.bits_per_sample as usize / 8).max(1);
    let block_align = (spec.channels as usize).max(1) * bytes_per_sample;
    let total_frames = (spec.data_size as usize) / block_align;
    let mut cursor_frame: usize = 0;
    let mut last_reported_sec: u64 = 0;
    let mut raw_buf = vec![0u8; CHUNK_FRAMES * block_align];

    while cursor_frame < total_frames {
        if control.generation.load(Ordering::Acquire) != gen {
            return Ok(false);
        }
        wait_if_paused(control, gen);
        if control.generation.load(Ordering::Acquire) != gen {
            return Ok(false);
        }

        let pending_seek = control.seek_ms.swap(-1, Ordering::AcqRel);
        if pending_seek >= 0 && spec.sample_rate > 0 {
            let target_frame =
                (((pending_seek as u64) * u64::from(spec.sample_rate)) / 1000) as usize;
            cursor_frame = target_frame.min(total_frames);
            let byte_pos = spec.data_offset + (cursor_frame * block_align) as u64;
            reader
                .seek(SeekFrom::Start(byte_pos))
                .map_err(|e| format!("wav seek: {e}"))?;
            last_reported_sec = ((cursor_frame as u64) * 1000 / u64::from(spec.sample_rate)) / 1000;
            delegate.notify_playback_state(pending_seek as u32);
            continue;
        }

        let frames_left = total_frames - cursor_frame;
        let to_read_frames = frames_left.min(CHUNK_FRAMES);
        let to_read_bytes = to_read_frames * block_align;
        reader
            .read_exact(&mut raw_buf[..to_read_bytes])
            .map_err(|e| format!("wav read: {e}"))?;
        cursor_frame += to_read_frames;

        let mut samples = Vec::with_capacity(to_read_frames * spec.channels as usize);
        for chunk in raw_buf[..to_read_bytes].chunks_exact(bytes_per_sample) {
            let sample = match (spec.audio_format, spec.bits_per_sample) {
                (1, 16) => i16::from_le_bytes([chunk[0], chunk[1]]) as f64 / 32768.0,
                (1, 24) => {
                    let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]) << 8 >> 8;
                    v as f64 / 8_388_608.0
                }
                (3, 32) => f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f64,
                _ => 0.0,
            };
            samples.push(sample);
        }

        let stereo = resample_interleaved_to_stereo_44100(
            &samples,
            spec.channels as usize,
            spec.sample_rate,
        );
        if !stereo.is_empty() {
            write_stereo_frames(stereo, volume_curve, control, sink, converter)?;
            let pos_ms = ((cursor_frame as u64) * 1000) / u64::from(spec.sample_rate.max(1));
            let sec = pos_ms / 1000;
            if sec > last_reported_sec {
                last_reported_sec = sec;
                delegate.notify_playback_state(pos_ms as u32);
            }
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use librespot::playback::audio_backend::SinkResult;

    #[derive(Clone, Default)]
    struct TestSink(Arc<Mutex<Vec<f64>>>);

    impl Sink for TestSink {
        fn start(&mut self) -> SinkResult<()> {
            Ok(())
        }
        fn stop(&mut self) -> SinkResult<()> {
            Ok(())
        }
        fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
            if let AudioPacket::Samples(s) = packet {
                self.0.lock().unwrap().extend_from_slice(&s);
            }
            Ok(())
        }
    }

    #[test]
    fn decodes_synthetic_wav_through_processor_chain_and_level_meter() {
        let path =
            std::env::temp_dir().join(format!("canto_stream_test_{}.wav", std::process::id()));
        let sample_rate = 22_050u32;
        let channels = 1u16;
        let frames = 2205usize;
        let data_bytes = (frames * 2) as u32;
        let mut wav = Vec::with_capacity(44 + data_bytes as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_bytes.to_le_bytes());
        for i in 0..frames {
            let val = (((i as f64) * 440.0 * std::f64::consts::TAU / sample_rate as f64).sin()
                * 16000.0) as i16;
            wav.extend_from_slice(&val.to_le_bytes());
        }
        std::fs::write(&path, wav).unwrap();

        let meter = LevelMeterController::new(true);
        let recorded = TestSink::default();
        let chain = ProcessorChain::new()
            .with(Box::new(EqProcessor::new(EqController::new([0.0; 10]))))
            .with(Box::new(MixProcessor::new(MixController::new(false))))
            .with(Box::new(LimiterProcessor::new()))
            .with(Box::new(LevelMeterProcessor::new(meter.clone())));
        let mut sink = CaptureSink::wrap(Box::new(recorded.clone()), chain);
        let mut converter = Converter::new(None);
        let (tx, _rx) = futures::channel::mpsc::unbounded();
        let delegate = AppPlayerDelegate::new(tx);
        let control = StreamControl::new(1.0);

        let f = File::open(&path).unwrap();
        let finished = stream_wav(
            f,
            0,
            VolumeCurveType::Linear,
            &control,
            &delegate,
            &mut *sink,
            &mut converter,
        )
        .unwrap();
        assert!(finished);

        let out = recorded.0.lock().unwrap();
        // 22050 Hz mono 0.1s resampled to 44100 Hz stereo ~= 4410 * 2 samples.
        assert!(out.len() >= 8000);
        let (rms, peak) = meter.levels();
        assert!(peak > 0.1 && rms > 0.05);

        let _ = std::fs::remove_file(&path);
    }
}
