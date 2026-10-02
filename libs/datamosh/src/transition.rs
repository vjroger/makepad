//! THE DATAMOSH TRANSITION: the cut where the incoming clip's keyframe
//! never arrives.
//!
//! An editor makes it by deleting the I-frame at the start of clip B: the
//! decoder keeps showing clip A's last picture and applies B's P-frames to
//! it, so A's pixels are dragged around by B's motion while B's residual
//! paints B's changes on top, until intra-coded blocks (and finally the
//! next keyframe) bring B in for real.
//!
//! Here that is a curve over the transition's progress:
//!
//! - `progress <= 0` ([`TransitionPhase::Before`]): clip A, clean. The
//!   motion history is already following the motion clip, so the first
//!   moshed step has real vectors;
//! - the first `hold` of the transition: pure P-frames on A's frozen last
//!   picture — nothing heals;
//! - the rest: intra refresh ramps up block by block toward B, and a last
//!   soft heal clears what is left of A;
//! - the final `fade_out` of the transition dissolves the output into the
//!   clean incoming clip on every display frame (not only on source
//!   frames), so it lands on B exactly, with nothing left to snap;
//! - `progress >= 1` ([`TransitionPhase::After`]): B's keyframe, clean.
//!
//! This module is the plan only (pure, no GPU); [`crate::Datamosh::drive_transition`]
//! applies it.

/// Whose motion moves the frozen picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransitionMotion {
    /// The incoming clip's motion moves the outgoing clip's last picture:
    /// the classic deleted-keyframe cut.
    #[default]
    Incoming,
    /// The outgoing clip keeps moving its own frozen last picture while the
    /// incoming clip heals in through it.
    Outgoing,
}

impl TransitionMotion {
    pub const ALL: &'static [TransitionMotion] =
        &[TransitionMotion::Incoming, TransitionMotion::Outgoing];

    pub const fn label(self) -> &'static str {
        match self {
            TransitionMotion::Incoming => "Incoming motion",
            TransitionMotion::Outgoing => "Outgoing motion",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionParams {
    /// Fraction of the transition that is pure mosh before the incoming
    /// clip starts to come through.
    pub hold: f32,
    /// Intra-refresh chance per block per step at the very end.
    pub refresh_peak: f32,
    /// The motion clip's residual while moshing. 1 is a faithful decoder:
    /// the incoming clip's changes paint over the frozen picture exactly as
    /// its P-frames would.
    pub residual: f32,
    /// Fraction at the end of the transition over which the output
    /// dissolves into the clean incoming clip.
    pub fade_out: f32,
    pub motion: TransitionMotion,
}

impl Default for TransitionParams {
    fn default() -> Self {
        Self {
            hold: 0.4,
            refresh_peak: 0.3,
            residual: 1.0,
            fade_out: 0.25,
            motion: TransitionMotion::Incoming,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionPhase {
    /// The outgoing clip, clean.
    Before,
    /// P-frames on the outgoing clip's last picture, healing into the
    /// incoming clip.
    Mosh,
    /// The incoming clip, clean.
    After,
}

/// What one display frame of the transition asks of the decoder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransitionFrame {
    pub phase: TransitionPhase,
    /// Intra-refresh chance per block per step (toward the incoming clip).
    pub refresh: f32,
    /// Soft per-step pull toward the incoming clip.
    pub heal: f32,
    /// Residual of the motion clip painted onto the reference.
    pub residual: f32,
    /// Output mix: 1 shows the mosh, 0 the clean incoming clip. Applied
    /// per display frame.
    pub wet: f32,
    /// Envelope on the output motion blurs: fades in over the start of the
    /// transition and out with `wet` at the end, 0 before and after.
    pub blur: f32,
}

impl TransitionParams {
    /// The plan at `progress` (0 = cut point, 1 = fully the incoming clip).
    /// NaN counts as not started.
    pub fn frame(&self, progress: f32) -> TransitionFrame {
        let clean = |phase| TransitionFrame {
            phase,
            refresh: 0.0,
            heal: 0.0,
            residual: 0.0,
            wet: 0.0,
            blur: 0.0,
        };
        if !(progress > 0.0) {
            return clean(TransitionPhase::Before);
        }
        if progress >= 1.0 {
            return clean(TransitionPhase::After);
        }
        let hold = self.hold.clamp(0.0, 0.99);
        let healing = ((progress - hold) / (1.0 - hold)).clamp(0.0, 1.0);
        let wet = 1.0 - smoothstep(1.0 - self.fade_out.clamp(1e-4, 1.0), 1.0, progress);
        TransitionFrame {
            phase: TransitionPhase::Mosh,
            // Ease in: the first refreshed blocks are rare islands of the
            // incoming clip, the end is a flood.
            refresh: self.refresh_peak.clamp(0.0, 1.0) * healing * healing,
            heal: smoothstep(0.8, 1.0, healing) * 0.5,
            residual: self.residual,
            wet,
            blur: smoothstep(0.0, 0.15, progress) * wet,
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
