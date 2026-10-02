//! Depth sources and the temporal stabilizer.
//!
//! Every source turns one NV12 frame into a [`RawDepth`] map at (about) the
//! requested resolution; [`Stabilizer`] turns that into the normalized
//! disparity map the point shader reads (1 = near plane, 0 = far plane,
//! negative = no depth). The speed / accuracy knob is the model input size
//! (`depth_res`, long side in pixels) plus how often the pipeline runs a
//! source at all (`depth_every`, see `pipeline.rs`).

/// Where the picture and the packed depth live inside a decoded frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameLayout {
    /// The whole frame is the picture.
    Full,
    /// RGBD side by side: picture left half, grayscale depth right half.
    SideBySide,
    /// RGBD over/under: picture top half, grayscale depth bottom half.
    TopBottom,
}

impl FrameLayout {
    /// `[u0, v0, du, dv]` of the picture in frame texture coordinates.
    pub fn picture_rect(self) -> [f32; 4] {
        match self {
            Self::Full => [0.0, 0.0, 1.0, 1.0],
            Self::SideBySide => [0.0, 0.0, 0.5, 1.0],
            Self::TopBottom => [0.0, 0.0, 1.0, 0.5],
        }
    }

    /// `[u0, v0, du, dv]` of the packed depth, when the layout carries one.
    pub fn depth_rect(self) -> Option<[f32; 4]> {
        match self {
            Self::Full => None,
            Self::SideBySide => Some([0.5, 0.0, 0.5, 1.0]),
            Self::TopBottom => Some([0.0, 0.5, 1.0, 0.5]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthKind {
    /// Larger is nearer (relative inverse depth: Depth-Anything, packed RGBD).
    Disparity,
    /// Larger is farther (metric depth); non-finite = sky / no depth.
    #[cfg(feature = "localai")]
    Depth,
}

pub struct RawDepth {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
    pub kind: DepthKind,
}

/// Normalized disparity: 1 = near plane, 0 = far plane, < 0 = no point.
pub struct DepthMap {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

/// A decoded frame: tightly packed NV12 (stride == width).
pub struct FrameView<'a> {
    pub width: usize,
    pub height: usize,
    pub nv12: &'a [u8],
}

impl FrameView<'_> {
    fn is_valid(&self) -> bool {
        self.width >= 2
            && self.height >= 2
            && self.nv12.len() >= self.width * self.height + (self.width / 2) * (self.height / 2) * 2
    }

    /// Pixel size of a `[u0, v0, du, dv]` region.
    fn rect_px(&self, rect: [f32; 4]) -> (f32, f32) {
        (self.width as f32 * rect[2], self.height as f32 * rect[3])
    }

    /// Box-filtered limited-range luma (0..1) of `rect`, resampled to `w x h`.
    pub fn luma(&self, rect: [f32; 4], w: usize, h: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(w * h);
        for oy in 0..h {
            let (y0, y1) = self.span(rect[1], rect[3], self.height, oy, h);
            for ox in 0..w {
                let (x0, x1) = self.span(rect[0], rect[2], self.width, ox, w);
                let mut sum = 0u32;
                for y in y0..y1 {
                    let row = &self.nv12[y * self.width..];
                    for x in x0..x1 {
                        sum += row[x] as u32;
                    }
                }
                let mean = sum as f32 / ((y1 - y0) * (x1 - x0)) as f32;
                out.push(((mean - 16.0) / 219.0).clamp(0.0, 1.0));
            }
        }
        out
    }

    /// Box-filtered BT.709 limited-range RGB of `rect`, resampled to `w x h`,
    /// as `[r, g, b]` in 0..1.
    #[cfg(feature = "localai")]
    pub fn rgb(&self, rect: [f32; 4], w: usize, h: usize) -> Vec<[f32; 3]> {
        let luma = self.luma(rect, w, h);
        let uv_plane = &self.nv12[self.width * self.height..];
        let mut out = Vec::with_capacity(w * h);
        for oy in 0..h {
            let (y0, y1) = self.span(rect[1], rect[3], self.height, oy, h);
            let cy = ((y0 + y1) / 2 / 2).min(self.height / 2 - 1);
            for ox in 0..w {
                let (x0, x1) = self.span(rect[0], rect[2], self.width, ox, w);
                let cx = ((x0 + x1) / 2 / 2).min(self.width / 2 - 1);
                let at = cy * self.width + cx * 2;
                let u = (uv_plane[at] as f32 - 128.0) / 224.0;
                let v = (uv_plane[at + 1] as f32 - 128.0) / 224.0;
                let y = luma[oy * w + ox];
                out.push([
                    (y + 1.5748 * v).clamp(0.0, 1.0),
                    (y - 0.1873 * u - 0.4681 * v).clamp(0.0, 1.0),
                    (y + 1.8556 * u).clamp(0.0, 1.0),
                ]);
            }
        }
        out
    }

    /// Source pixel range `[a, b)` covered by output cell `i` of `n`.
    fn span(&self, start: f32, size: f32, extent: usize, i: usize, n: usize) -> (usize, usize) {
        let px0 = start * extent as f32;
        let px = size * extent as f32;
        let a = (px0 + px * i as f32 / n as f32).floor() as usize;
        let b = (px0 + px * (i + 1) as f32 / n as f32).ceil() as usize;
        let a = a.min(extent - 1);
        (a, b.clamp(a + 1, extent))
    }
}

/// Fit `src_w x src_h` into `long_side`, both sides snapped to `multiple`.
pub fn fit_dims(src_w: f32, src_h: f32, long_side: usize, multiple: usize) -> (usize, usize) {
    let multiple = multiple.max(1);
    let long_side = long_side.max(multiple);
    let scale = long_side as f32 / src_w.max(src_h).max(1.0);
    let snap = |v: f32| (((v / multiple as f32).round() as usize).max(1)) * multiple;
    (snap(src_w * scale), snap(src_h * scale))
}

pub trait DepthEstimator {
    fn label(&self) -> String;
    /// Depth for the `picture` region of `frame`, the long side near `depth_res`.
    fn estimate(
        &mut self,
        frame: &FrameView,
        picture: [f32; 4],
        depth_res: usize,
    ) -> Result<RawDepth, String>;
}

/// What the worker builds its estimator from (the estimator itself is
/// created on the worker thread: model state need not be `Send`).
#[derive(Clone, Debug)]
pub enum DepthSource {
    /// Constant depth: the video as a flat plane (pipeline / renderer check).
    Flat,
    /// Zero-cost "ground plane" prior: lower in the frame is nearer.
    GroundPrior,
    /// Depth packed in the frame itself (RGBD side-by-side / top-bottom).
    Packed(FrameLayout),
    /// Native Depth-Anything-V2 family, e.g. V2-Small: the realtime tier
    /// (`--features localai`).
    #[cfg(feature = "localai")]
    Anything { model_path: String },
    /// Native Depth-Anything-3 metric-large (`--features localai`).
    #[cfg(feature = "localai")]
    Da3 { model_path: String },
}

impl DepthSource {
    pub fn build(&self) -> Result<Box<dyn DepthEstimator>, String> {
        Ok(match self {
            Self::Flat => Box::new(FlatDepth),
            Self::GroundPrior => Box::new(GroundPrior),
            Self::Packed(layout) => Box::new(PackedDepth {
                rect: layout
                    .depth_rect()
                    .ok_or("packed depth needs a side-by-side or top-bottom layout")?,
            }),
            #[cfg(feature = "localai")]
            Self::Anything { model_path } => Box::new(native::AnythingDepth::load(model_path)?),
            #[cfg(feature = "localai")]
            Self::Da3 { model_path } => Box::new(native::Da3Depth::load(model_path)?),
        })
    }
}

struct FlatDepth;

impl DepthEstimator for FlatDepth {
    fn label(&self) -> String {
        "flat".into()
    }

    fn estimate(&mut self, _: &FrameView, _: [f32; 4], _: usize) -> Result<RawDepth, String> {
        Ok(RawDepth {
            width: 2,
            height: 2,
            values: vec![1.0; 4],
            kind: DepthKind::Disparity,
        })
    }
}

struct GroundPrior;

impl DepthEstimator for GroundPrior {
    fn label(&self) -> String {
        "ground prior".into()
    }

    fn estimate(&mut self, _: &FrameView, _: [f32; 4], _: usize) -> Result<RawDepth, String> {
        let (w, h) = (2, 64);
        let mut values = Vec::with_capacity(w * h);
        for y in 0..h {
            let t = (y as f32 + 0.5) / h as f32;
            let d = 0.15 + 0.85 * t * t;
            values.extend([d; 2]);
        }
        Ok(RawDepth {
            width: w,
            height: h,
            values,
            kind: DepthKind::Disparity,
        })
    }
}

/// Grayscale depth packed next to the picture: white = near.
struct PackedDepth {
    rect: [f32; 4],
}

impl DepthEstimator for PackedDepth {
    fn label(&self) -> String {
        "packed RGBD".into()
    }

    fn estimate(
        &mut self,
        frame: &FrameView,
        _picture: [f32; 4],
        depth_res: usize,
    ) -> Result<RawDepth, String> {
        if !frame.is_valid() {
            return Err("frame too small".into());
        }
        let (sw, sh) = frame.rect_px(self.rect);
        let (w, h) = fit_dims(sw, sh, depth_res.min(sw.max(sh) as usize), 1);
        Ok(RawDepth {
            width: w,
            height: h,
            values: frame.luma(self.rect, w, h),
            kind: DepthKind::Disparity,
        })
    }
}

#[cfg(feature = "localai")]
mod native {
    use super::*;
    use makepad_ai_vision::da3::{Da3MetricLarge, DA3_PATCH};
    use makepad_ai_vision::depth_anything::DepthAnything;

    const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];
    /// DA3's sky-head threshold (`depth_backend.rs` / `encode_metric_mm`).
    const SKY_THRESHOLD: f32 = 0.3;

    /// The picture region resized to a 14-multiple and ImageNet-normalized
    /// into planar RGB.
    fn network_input(
        frame: &FrameView,
        picture: [f32; 4],
        depth_res: usize,
    ) -> Result<(Vec<f32>, usize, usize), String> {
        if !frame.is_valid() {
            return Err("frame too small".into());
        }
        let (sw, sh) = frame.rect_px(picture);
        let (w, h) = fit_dims(sw, sh, depth_res, DA3_PATCH);
        let rgb = frame.rgb(picture, w, h);
        let plane = w * h;
        let mut pixels = vec![0.0f32; 3 * plane];
        for (i, px) in rgb.iter().enumerate() {
            for c in 0..3 {
                pixels[c * plane + i] = (px[c] - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
            }
        }
        Ok((pixels, w, h))
    }

    pub struct AnythingDepth {
        model: DepthAnything,
    }

    impl AnythingDepth {
        pub fn load(path: &str) -> Result<Self, String> {
            let model = DepthAnything::load(path).map_err(|e| format!("Depth-Anything load {path}: {e}"))?;
            Ok(Self { model })
        }
    }

    impl DepthEstimator for AnythingDepth {
        fn label(&self) -> String {
            format!("Depth-Anything {} (native)", self.model.config().variant())
        }

        fn estimate(
            &mut self,
            frame: &FrameView,
            picture: [f32; 4],
            depth_res: usize,
        ) -> Result<RawDepth, String> {
            let (pixels, w, h) = network_input(frame, picture, depth_res)?;
            let prediction = self
                .model
                .forward_normalized(&pixels, w, h)
                .map_err(|e| format!("Depth-Anything: {e}"))?;
            Ok(RawDepth {
                width: prediction.width,
                height: prediction.height,
                values: prediction.disparity,
                kind: DepthKind::Disparity,
            })
        }
    }

    pub struct Da3Depth {
        model: Da3MetricLarge,
    }

    impl Da3Depth {
        pub fn load(path: &str) -> Result<Self, String> {
            let model = Da3MetricLarge::load(path).map_err(|e| format!("DA3 load {path}: {e}"))?;
            Ok(Self { model })
        }
    }

    impl DepthEstimator for Da3Depth {
        fn label(&self) -> String {
            "DA3 metric-large (native)".into()
        }

        fn estimate(
            &mut self,
            frame: &FrameView,
            picture: [f32; 4],
            depth_res: usize,
        ) -> Result<RawDepth, String> {
            let (pixels, w, h) = network_input(frame, picture, depth_res)?;
            let prediction = self
                .model
                .forward_normalized(&pixels, w, h, None)
                .map_err(|e| format!("DA3: {e}"))?;
            let values = prediction
                .depth
                .iter()
                .zip(&prediction.sky)
                .map(|(&z, &sky)| if sky >= SKY_THRESHOLD { f32::INFINITY } else { z })
                .collect();
            Ok(RawDepth {
                width: prediction.width,
                height: prediction.height,
                values,
                kind: DepthKind::Depth,
            })
        }
    }
}

/// Raw map -> normalized disparity, stabilized over time.
///
/// Relative models are only defined up to scale and shift per frame, so the
/// map is normalized by its robust (2nd..98th percentile) disparity range.
/// Smoothing that RANGE over time removes most flicker without the ghosting
/// a per-pixel blend causes on motion; the per-pixel blend is still offered.
#[derive(Default)]
pub struct Stabilizer {
    range: Option<(f32, f32)>,
    prev: Vec<f32>,
    prev_dims: (usize, usize),
}

impl Stabilizer {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn apply(&mut self, raw: RawDepth, range_smoothing: f32, pixel_smoothing: f32) -> DepthMap {
        let disparity: Vec<f32> = match raw.kind {
            DepthKind::Disparity => raw.values,
            #[cfg(feature = "localai")]
            DepthKind::Depth => raw
                .values
                .into_iter()
                .map(|z| {
                    if z.is_nan() {
                        z
                    } else if z.is_finite() && z > 0.0 {
                        1.0 / z
                    } else {
                        0.0
                    }
                })
                .collect(),
        };

        let step = (disparity.len() / 16384).max(1);
        let mut samples: Vec<f32> = disparity
            .iter()
            .step_by(step)
            .copied()
            .filter(|d| d.is_finite() && *d > 0.0)
            .collect();
        let (mut lo, mut hi) = if samples.len() >= 4 {
            (percentile(&mut samples, 0.02), percentile(&mut samples, 0.98))
        } else {
            (0.0, 1.0)
        };
        if let Some((prev_lo, prev_hi)) = self.range {
            let k = range_smoothing.clamp(0.0, 0.99);
            lo = prev_lo * k + lo * (1.0 - k);
            hi = prev_hi * k + hi * (1.0 - k);
        }
        self.range = Some((lo, hi));

        let span = hi - lo;
        let mut values: Vec<f32> = disparity
            .iter()
            .map(|&d| {
                if !d.is_finite() {
                    -1.0
                } else if d <= 0.0 {
                    0.0
                } else if span > 1e-6 {
                    ((d - lo) / span).clamp(0.0, 1.0)
                } else {
                    0.5
                }
            })
            .collect();

        let k = pixel_smoothing.clamp(0.0, 0.95);
        if k > 0.0 && self.prev_dims == (raw.width, raw.height) && self.prev.len() == values.len() {
            for (v, p) in values.iter_mut().zip(&self.prev) {
                if *v >= 0.0 && *p >= 0.0 {
                    *v = p * k + *v * (1.0 - k);
                }
            }
        }
        self.prev.clone_from(&values);
        self.prev_dims = (raw.width, raw.height);

        DepthMap {
            width: raw.width,
            height: raw.height,
            values,
        }
    }
}

fn percentile(samples: &mut [f32], p: f32) -> f32 {
    let i = ((samples.len() - 1) as f32 * p).round() as usize;
    *samples.select_nth_unstable_by(i, f32::total_cmp).1
}
