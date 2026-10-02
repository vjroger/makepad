//! The knobs: what the decoder does with each motion step, and how a
//! renderer's own vector texture is read.

/// What the reference buffer holds, and so what a motion step moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MoshMode {
    /// A real decoder: the reference PICTURE is moved by every step and
    /// degrades the way a P-frame chain does — sub-pixel resampling smears
    /// it, the motion source's residual paints over it, damaged blocks
    /// break up. The datamosh transition always runs in this mode.
    #[default]
    Decode,
    /// The reference holds COORDINATES into the keyframe instead of
    /// colours (the reprojection trick): every step moves where each pixel
    /// samples from, and the keyframe is read once per frame, so a smear
    /// stays sharp however long it runs.
    Remap,
    /// As [`MoshMode::Remap`], but the moved coordinates sample the LIVE
    /// picture: it keeps playing inside the geometry the other source's
    /// motion has dragged it into.
    RemapLive,
}

impl MoshMode {
    pub const ALL: &'static [MoshMode] = &[MoshMode::Decode, MoshMode::Remap, MoshMode::RemapLive];

    pub const fn label(self) -> &'static str {
        match self {
            MoshMode::Decode => "Decode",
            MoshMode::Remap => "Remap",
            MoshMode::RemapLive => "Remap live",
        }
    }

    pub const fn is_remap(self) -> bool {
        matches!(self, MoshMode::Remap | MoshMode::RemapLive)
    }
}

/// The shape of the constant push added to every vector each step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DriftMode {
    /// Everything slides sideways (positive: right).
    #[default]
    Horizontal,
    /// Everything slides vertically (positive: down).
    Vertical,
    /// Turns around the frame centre (positive: clockwise).
    Rotate,
    /// Flows out of the frame centre (positive) or into it (negative).
    Zoom,
    /// Rotate and zoom together: a whirlpool.
    Spiral,
    /// Every block keeps sliding its own random way.
    Random,
}

impl DriftMode {
    pub const ALL: &'static [DriftMode] = &[
        DriftMode::Horizontal,
        DriftMode::Vertical,
        DriftMode::Rotate,
        DriftMode::Zoom,
        DriftMode::Spiral,
        DriftMode::Random,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            DriftMode::Horizontal => "Horizontal",
            DriftMode::Vertical => "Vertical",
            DriftMode::Rotate => "Rotate",
            DriftMode::Zoom => "Zoom",
            DriftMode::Spiral => "Spiral",
            DriftMode::Random => "Random",
        }
    }

    /// The code the step shader branches on.
    pub(crate) const fn code(self) -> f32 {
        match self {
            DriftMode::Horizontal => 0.0,
            DriftMode::Vertical => 1.0,
            DriftMode::Rotate => 2.0,
            DriftMode::Zoom => 3.0,
            DriftMode::Spiral => 4.0,
            DriftMode::Random => 5.0,
        }
    }
}

/// What the output texture shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MoshView {
    /// The effect.
    #[default]
    Output,
    /// The motion field the last step used: hue is direction, brightness
    /// is speed.
    Vectors,
    /// Accumulated codec damage per pixel (black = clean).
    Damage,
}

impl MoshView {
    pub const ALL: &'static [MoshView] = &[MoshView::Output, MoshView::Vectors, MoshView::Damage];

    pub const fn label(self) -> &'static str {
        match self {
            MoshView::Output => "Output",
            MoshView::Vectors => "Vectors",
            MoshView::Damage => "Damage",
        }
    }
}

/// The per-step decoder controls. Every distance is in OUTPUT pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoshParams {
    pub mode: MoshMode,
    /// Macroblock edge. Every pixel of a block moves by the block's one
    /// vector, as in MPEG: 16 is the classic look, 1 lets every pixel move
    /// on its own (a fluid smear).
    pub block_size: f32,
    /// Multiplies every vector. Above 1 exaggerates the motion; negative
    /// plays it backwards.
    pub gain: f32,
    /// 2x2 applied to each vector before `gain`, row major:
    /// `[xx, xy, yx, yy]`. Identity by default; `[-1, 0, 0, 1]` mirrors the
    /// motion horizontally, `[0, 1, 1, 0]` swaps the axes, a rotation
    /// turns it.
    pub matrix: [f32; 4],
    /// A push added to every vector each step, in pixels per step (at the
    /// frame edge for the radial patterns); 0 is off, negative reverses it.
    pub drift: f32,
    /// The pattern of that push.
    pub drift_mode: DriftMode,
    /// Random per-block jitter added to the vectors each step.
    pub diffusion: f32,
    /// Sub-pixel precision of the decoder: 0 is continuous, 1 whole pixels
    /// (crisp, small motion is lost), 2 half-pel, 4 quarter-pel (H.264).
    pub pel: f32,
    /// Chance per block per step that the block is intra-refreshed from
    /// the picture: 0 never heals, 1 is a keyframe every step.
    pub refresh: f32,
    /// Per-step pull of every pixel back toward the picture.
    pub heal: f32,
    /// How much of the motion source's own frame-to-frame change (the
    /// P-frame residual) is painted onto the reference. 1 with the
    /// picture as motion source is an exact decoder; on another source it
    /// ghosts that source's detail through. Decode mode only.
    pub residual: f32,
    /// Codec damage: DCT-pattern breakup and displaced blocks where the
    /// motion is large or the match was poor.
    pub entropy: f32,
    /// Mix with the clean picture: 0 dry, 1 fully moshed.
    pub wet: f32,
    pub view: MoshView,
}

impl Default for MoshParams {
    fn default() -> Self {
        Self {
            mode: MoshMode::Decode,
            block_size: 16.0,
            gain: 1.0,
            matrix: [1.0, 0.0, 0.0, 1.0],
            drift: 0.0,
            drift_mode: DriftMode::Horizontal,
            diffusion: 0.0,
            pel: 4.0,
            refresh: 0.0,
            heal: 0.0,
            residual: 0.0,
            entropy: 0.0,
            wet: 1.0,
            view: MoshView::Output,
        }
    }
}

/// How a supplied vector texture's RG channels are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VectorKind {
    /// Current minus previous position: how far the content now here has
    /// moved since the previous frame (Unity, Three.js velocity passes).
    #[default]
    Forward,
    /// Previous minus current position.
    Backward,
    /// The absolute position the content now here had in the previous
    /// frame (a reprojection buffer, like the classic shadertoy's).
    PreviousPosition,
}

/// Reads a renderer's own motion output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFormat {
    pub kind: VectorKind,
    /// Multiplies RG into uv units: `[1, 1]` for uv, `[1/w, 1/h]` for
    /// pixels, `[0.5, 0.5]` for NDC deltas.
    pub scale: [f32; 2],
    /// The texture's y axis points up (GL / NDC convention); the engine's
    /// uv space has y down.
    pub y_up: bool,
}

impl VectorFormat {
    /// Vectors already in top-left uv units.
    pub const fn uv(kind: VectorKind) -> Self {
        Self {
            kind,
            scale: [1.0, 1.0],
            y_up: false,
        }
    }

    /// Vectors in pixels of a `width` x `height` image, y down.
    pub fn pixels(kind: VectorKind, width: u32, height: u32) -> Self {
        Self {
            kind,
            scale: [1.0 / width.max(1) as f32, 1.0 / height.max(1) as f32],
            y_up: false,
        }
    }
}

impl Default for VectorFormat {
    fn default() -> Self {
        Self::uv(VectorKind::Forward)
    }
}
