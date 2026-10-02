//! The worker: decode -> pace -> depth -> stabilize -> hand to the UI.
//!
//! One long-lived pool task owns the decoder AND the depth model (the native
//! models keep thread-local device graphs, so inference must stay on one
//! thread). It paces frames against a wall clock and drops frames that are
//! already late, so playback stays realtime whatever the model costs: a slow
//! model lowers the shown frame rate instead of slowing the video down.
//! Depth runs every `depth_every` shown frames; the frames in between reuse
//! the previous map. Colour stays NV12 all the way to the GPU.
//!
//! The UI thread only does `try_recv` / `try_send` and atomics.

use crate::depth::{DepthMap, DepthSource, FrameLayout, FrameView, Stabilizer};
use makepad_widgets::log;
use makepad_widgets::makepad_platform::thread::{
    CancellationToken, Lane, TaskHandle, TaskPool, WaitOutcome,
};
use makepad_widgets::makepad_platform::video_file::VideoFileDecoder;
use makepad_widgets::Cx;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;

/// A frame later than this behind the clock is decoded but not shown.
const LATE_SECS: f64 = 0.040;
/// Never drop more than this many frames in a row (a decoder slower than
/// realtime would otherwise never show anything).
const MAX_CONSECUTIVE_DROPS: u32 = 12;

#[derive(Clone, Copy, Debug)]
pub struct PipelineSettings {
    /// Long side of the depth input in pixels: THE speed / accuracy knob.
    pub depth_res: usize,
    /// Run depth on every Nth shown frame; the rest reuse the last map.
    pub depth_every: u32,
    /// Weight of the previous frame's disparity range (scale/shift EMA).
    pub range_smoothing: f32,
    /// Weight of the previous frame's per-pixel depth (ghosts on motion).
    pub pixel_smoothing: f32,
}

impl Default for PipelineSettings {
    fn default() -> Self {
        Self {
            depth_res: 308,
            depth_every: 1,
            range_smoothing: 0.85,
            pixel_smoothing: 0.0,
        }
    }
}

#[derive(Clone)]
pub enum VideoInput {
    File(String),
    /// Built-in animated RGBD clip (side-by-side, exact depth).
    Synthetic,
}

#[derive(Clone)]
pub struct SourceSpec {
    pub input: VideoInput,
    pub layout: FrameLayout,
    pub depth: DepthSource,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub shown: u64,
    pub dropped: u64,
    pub depth_runs: u64,
    pub depth_ms: f32,
    pub depth_width: usize,
    pub depth_height: usize,
}

pub struct CloudFrame {
    pub width: usize,
    pub height: usize,
    pub nv12: Vec<u8>,
    /// `None`: keep showing the previous depth map.
    pub depth: Option<DepthMap>,
    pub stats: FrameStats,
}

struct Shared {
    stop: AtomicBool,
    paused: AtomicBool,
    /// The worker gave up (its last status message says why).
    failed: AtomicBool,
}

pub struct Pipeline {
    shared: Arc<Shared>,
    frame_rx: Receiver<CloudFrame>,
    settings_tx: SyncSender<PipelineSettings>,
    pending_settings: Option<PipelineSettings>,
    status_rx: Receiver<String>,
    task: Option<TaskHandle<()>>,
    wait: CancellationToken,
}

impl Pipeline {
    pub fn start(
        pool: TaskPool,
        spec: SourceSpec,
        settings: PipelineSettings,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        });
        let (frame_tx, frame_rx) = sync_channel(2);
        let (settings_tx, settings_rx) = sync_channel(8);
        let (status_tx, status_rx) = sync_channel(16);
        let wait = CancellationToken::new();
        let worker = Worker {
            shared: shared.clone(),
            frame_tx,
            settings_rx,
            status_tx,
            wait: wait.clone(),
            settings,
        };
        let task = pool
            .submit(Lane::Heavy, move || worker.run(spec))
            .map_err(|e| format!("could not queue the depth-cloud worker: {e}"))?;
        Ok(Self {
            shared,
            frame_rx,
            settings_tx,
            pending_settings: None,
            status_rx,
            task: Some(task),
            wait,
        })
    }

    /// The newest frame, merging the depth of any older frame skipped here.
    pub fn take_frame(&mut self) -> Option<CloudFrame> {
        let mut newest: Option<CloudFrame> = None;
        loop {
            match self.frame_rx.try_recv() {
                Ok(mut frame) => {
                    if frame.depth.is_none() {
                        frame.depth = newest.take().and_then(|older| older.depth);
                    }
                    newest = Some(frame);
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => return newest,
            }
        }
    }

    pub fn take_status(&mut self) -> Option<String> {
        let mut last = None;
        while let Ok(status) = self.status_rx.try_recv() {
            last = Some(status);
        }
        last
    }

    pub fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Acquire)
    }

    /// Queue new settings; a full queue keeps them for [`Self::flush_settings`].
    pub fn set_settings(&mut self, settings: PipelineSettings) {
        self.pending_settings = Some(settings);
        self.flush_settings();
    }

    pub fn flush_settings(&mut self) {
        if let Some(settings) = self.pending_settings.take() {
            if let Err(TrySendError::Full(settings)) = self.settings_tx.try_send(settings) {
                self.pending_settings = Some(settings);
            }
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Release);
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        // Never join: the worker sees `stop` between frames and exits.
        self.shared.stop.store(true, Ordering::Release);
        self.wait.cancel();
        if let Some(task) = self.task.take() {
            task.cancel();
        }
    }
}

struct SourceFrame {
    pts_100ns: i64,
    width: usize,
    height: usize,
    nv12: Vec<u8>,
}

trait FrameSource {
    fn next_frame(&mut self) -> Result<Option<SourceFrame>, String>;
    fn rewind(&mut self) -> Result<(), String>;
}

struct FileSource(VideoFileDecoder);

impl FrameSource for FileSource {
    fn next_frame(&mut self) -> Result<Option<SourceFrame>, String> {
        Ok(self.0.next_frame().map_err(|e| e.to_string())?.map(|f| SourceFrame {
            pts_100ns: f.pts_100ns,
            width: f.width as usize,
            height: f.height as usize,
            nv12: f.nv12,
        }))
    }

    fn rewind(&mut self) -> Result<(), String> {
        self.0.seek(0).map_err(|e| e.to_string())
    }
}

struct Worker {
    shared: Arc<Shared>,
    frame_tx: SyncSender<CloudFrame>,
    settings_rx: Receiver<PipelineSettings>,
    status_tx: SyncSender<String>,
    wait: CancellationToken,
    settings: PipelineSettings,
}

impl Worker {
    fn status(&self, text: String) {
        log!("depth-cloud: {text}");
        let _ = self.status_tx.try_send(text);
    }

    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Acquire)
    }

    fn run(mut self, spec: SourceSpec) {
        if let Err(err) = self.run_inner(spec) {
            // Status first: once the UI sees `failed` it reads the reason and stops polling.
            self.status(err);
            self.shared.failed.store(true, Ordering::Release);
        }
    }

    fn run_inner(&mut self, spec: SourceSpec) -> Result<(), String> {
        let mut source: Box<dyn FrameSource> = match &spec.input {
            VideoInput::File(path) => {
                let decoder = VideoFileDecoder::open(path).map_err(|e| format!("{path}: {e}"))?;
                let info = decoder.info();
                self.status(format!(
                    "{path}: {}x{} @ {:.2} fps",
                    info.width,
                    info.height,
                    info.fps_num as f64 / info.fps_den.max(1) as f64
                ));
                Box::new(FileSource(decoder))
            }
            VideoInput::Synthetic => Box::new(SyntheticSource::default()),
        };
        let mut depth_pass: Option<Box<dyn FrameSource>> = match spec.depth.depth_pass_video() {
            Some(path) => Some(Box::new(FileSource(
                VideoFileDecoder::open(path).map_err(|e| format!("depth pass {path}: {e}"))?,
            ))),
            None => None,
        };
        self.status(format!("loading depth source {:?}", spec.depth));
        let mut estimator = spec.depth.build()?;
        self.status(format!("depth: {}", estimator.label()));

        let picture = spec.layout.picture_rect();
        let mut stabilizer = Stabilizer::default();
        let mut stats = FrameStats::default();
        // (wall time, pts) of the clock origin.
        let mut clock: Option<(f64, i64)> = None;
        let mut paused_at: Option<f64> = None;
        let mut since_depth = u32::MAX;
        let mut drops_in_row = 0u32;
        let mut depth_errors = 0u32;

        loop {
            if self.stopped() {
                return Ok(());
            }
            while let Ok(settings) = self.settings_rx.try_recv() {
                if settings.depth_res != self.settings.depth_res {
                    since_depth = u32::MAX;
                }
                self.settings = settings;
            }
            let now = Cx::monotonic_now();
            if self.shared.paused.load(Ordering::Acquire) {
                paused_at.get_or_insert(now);
                if matches!(self.wait.wait_until(now + 0.02), WaitOutcome::Cancelled) {
                    return Ok(());
                }
                continue;
            }
            if let (Some(at), Some(origin)) = (paused_at.take(), clock.as_mut()) {
                origin.0 += now - at;
            }

            let frame = match source.next_frame()? {
                Some(frame) => frame,
                None => {
                    // Loop the clip.
                    source.rewind()?;
                    if let Some(pass) = depth_pass.as_mut() {
                        pass.rewind()?;
                    }
                    clock = None;
                    stabilizer.reset();
                    since_depth = u32::MAX;
                    continue;
                }
            };
            // The depth pass advances with the picture, dropped frames included.
            let pass_frame = match depth_pass.as_mut() {
                Some(pass) => pass.next_frame()?,
                None => None,
            };
            let (wall0, pts0) = *clock.get_or_insert((now, frame.pts_100ns));
            let due = wall0 + (frame.pts_100ns - pts0) as f64 * 1e-7;
            if now > due + LATE_SECS && drops_in_row < MAX_CONSECUTIVE_DROPS {
                drops_in_row += 1;
                stats.dropped += 1;
                continue;
            }
            drops_in_row = 0;

            let mut depth = None;
            let depth_frame = match (&depth_pass, &pass_frame) {
                (None, _) => Some(&frame),
                (Some(_), pass) => pass.as_ref(),
            };
            if let Some(depth_frame) = depth_frame.filter(|_| since_depth >= self.settings.depth_every.max(1)) {
                let view = FrameView {
                    width: depth_frame.width,
                    height: depth_frame.height,
                    nv12: &depth_frame.nv12,
                };
                let started = Cx::monotonic_now();
                match estimator.estimate(&view, picture, self.settings.depth_res) {
                    Ok(raw) => {
                        stats.depth_ms = ((Cx::monotonic_now() - started) * 1000.0) as f32;
                        stats.depth_runs += 1;
                        stats.depth_width = raw.width;
                        stats.depth_height = raw.height;
                        depth = Some(stabilizer.apply(
                            raw,
                            self.settings.range_smoothing,
                            self.settings.pixel_smoothing,
                        ));
                        since_depth = 0;
                    }
                    Err(err) => {
                        depth_errors += 1;
                        if depth_errors <= 3 {
                            self.status(format!("depth failed: {err}"));
                        }
                    }
                }
            }
            since_depth = since_depth.saturating_add(1);

            if matches!(self.wait.wait_until(due), WaitOutcome::Cancelled) {
                return Ok(());
            }
            stats.shown += 1;
            let mut out = CloudFrame {
                width: frame.width,
                height: frame.height,
                nv12: frame.nv12,
                depth,
                stats,
            };
            loop {
                match self.frame_tx.try_send(out) {
                    Ok(()) => break,
                    Err(TrySendError::Full(back)) => {
                        out = back;
                        let retry = Cx::monotonic_now() + 0.004;
                        if self.stopped()
                            || matches!(self.wait.wait_until(retry), WaitOutcome::Cancelled)
                        {
                            return Ok(());
                        }
                    }
                    Err(TrySendError::Disconnected(_)) => return Ok(()),
                }
            }
        }
    }
}

/// An animated side-by-side RGBD test clip with exact depth: a checkered
/// back wall, a floor and a bouncing sphere. Exercises the whole pipeline
/// (packed depth, pacing, renderer) without a video file or a model.
struct SyntheticSource {
    frame: i64,
}

impl Default for SyntheticSource {
    fn default() -> Self {
        Self { frame: 0 }
    }
}

impl SyntheticSource {
    const W: usize = 640;
    const H: usize = 360;
    const FPS: i64 = 30;

    /// Colour and disparity (1 = near) at picture coordinates `u, v` in 0..1.
    fn shade(u: f32, v: f32, t: f32) -> ([f32; 3], f32) {
        let aspect = Self::W as f32 / Self::H as f32;
        let x = (u * 2.0 - 1.0) * aspect;
        let y = 1.0 - v * 2.0;
        let (sx, sy, r) = (0.9 * (t * 0.7).sin(), -0.25 + 0.35 * (t * 1.9).sin().abs(), 0.42);
        let (dx, dy) = (x - sx, y - sy);
        let rr = dx * dx + dy * dy;
        if rr < r * r {
            let nz = (1.0 - rr / (r * r)).sqrt();
            let light = (0.25 + 0.75 * (0.5 * nz - 0.4 * dx / r + 0.5 * dy / r).max(0.0)).min(1.0);
            return ([0.95 * light, 0.55 * light, 0.2 * light], 0.62 + 0.25 * nz);
        }
        if y < -0.35 {
            // Floor: nearer toward the bottom of the frame.
            let s = (-0.35 - y) / 0.65;
            let checker = ((x / (0.25 + 0.6 * s)).floor() as i32 + (s * 8.0) as i32) & 1;
            let c = if checker == 0 { 0.30 } else { 0.55 };
            return ([c * 0.7, c * 0.8, c], 0.2 + 0.75 * s);
        }
        let checker = (((x * 4.0).floor() + (y * 4.0).floor()) as i32) & 1;
        let c = if checker == 0 { 0.2 } else { 0.8 };
        ([c, c * 0.95, c * 0.9], 0.2)
    }
}

impl FrameSource for SyntheticSource {
    fn next_frame(&mut self) -> Result<Option<SourceFrame>, String> {
        let (w, h) = (Self::W * 2, Self::H);
        let t = self.frame as f32 / Self::FPS as f32;
        let mut rgb = vec![[0.0f32; 3]; Self::W * h];
        let mut disparity = vec![0.0f32; Self::W * h];
        for py in 0..h {
            for px in 0..Self::W {
                let u = (px as f32 + 0.5) / Self::W as f32;
                let v = (py as f32 + 0.5) / h as f32;
                let (c, d) = Self::shade(u, v, t);
                rgb[py * Self::W + px] = c;
                disparity[py * Self::W + px] = d;
            }
        }
        let mut nv12 = vec![128u8; w * h + w * h / 2];
        let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let to_y = |l: f32| (16.0 + 219.0 * l.clamp(0.0, 1.0)).round() as u8;
        for py in 0..h {
            for px in 0..Self::W {
                let i = py * Self::W + px;
                nv12[py * w + px] = to_y(luma(rgb[i]));
                nv12[py * w + Self::W + px] = to_y(disparity[i]);
            }
        }
        // Chroma for the picture half; the depth half stays neutral (128).
        for cy in 0..h / 2 {
            for cx in 0..Self::W / 2 {
                let mut sum = [0.0f32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let c = rgb[(cy * 2 + dy) * Self::W + cx * 2 + dx];
                    for k in 0..3 {
                        sum[k] += c[k] * 0.25;
                    }
                }
                let l = luma(sum);
                let u = 128.0 + 224.0 * (sum[2] - l) / 1.8556;
                let v = 128.0 + 224.0 * (sum[0] - l) / 1.5748;
                let at = w * h + cy * w + cx * 2;
                nv12[at] = u.round().clamp(0.0, 255.0) as u8;
                nv12[at + 1] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
        let pts_100ns = self.frame * 10_000_000 / Self::FPS;
        self.frame += 1;
        if self.frame >= Self::FPS * 20 {
            return Ok(None);
        }
        Ok(Some(SourceFrame {
            pts_100ns,
            width: w,
            height: h,
            nv12,
        }))
    }

    fn rewind(&mut self) -> Result<(), String> {
        self.frame = 0;
        Ok(())
    }
}
