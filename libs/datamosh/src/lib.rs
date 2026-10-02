//! DATAMOSH as a realtime GPU effect: the look of a video whose keyframes
//! went missing, used on purpose to put the MOTION of one source onto the
//! PICTURE of another.
//!
//! A compressed video is mostly P-frames: per block, "take this block of
//! the previous picture from over there" (a motion vector) plus a small
//! correction (the residual). Drop the I-frame that should start a new shot
//! and the decoder keeps applying the new shot's vectors to the old shot's
//! pixels — the old picture melts along the new one's motion. This crate is
//! that decoder, with the vectors coming from anywhere:
//!
//! - **any texture sequence** — a clip, an animation, a camera, a live
//!   render — through an MPEG-style block-matching motion estimator that
//!   runs as GPU passes on the textures themselves (no readback);
//! - **a renderer's own vectors** (a velocity or reprojection buffer):
//!   exact motion, and the residual of the render itself if its picture is
//!   supplied too ([`Datamosh::push_motion_vectors`]).
//!
//! Three ways to accumulate ([`MoshMode`]): `Decode` moves the picture
//! itself and degrades like a real P-frame chain; `Remap` moves coordinates
//! into the keyframe so long smears stay sharp; `RemapLive` moves
//! coordinates into the live picture, which keeps playing inside the
//! dragged geometry.
//!
//! The datamosh TRANSITION ([`TransitionParams`],
//! [`Datamosh::drive_transition`]) is the deleted-keyframe cut: the incoming
//! clip's motion and residual drive the outgoing clip's last picture until
//! intra-refreshed blocks bring the incoming clip in.
//!
//! # Shape
//!
//! [`Datamosh`] is a component a host widget embeds as a `#[live]` field
//! and renders from its own `draw_walk`; [`DatamoshView`] wraps it as a
//! widget for a Splash tree. Neither draws on screen: the result is
//! [`Datamosh::output_texture`].
//!
//! ```ignore
//! // every display frame
//! mosh.set_frame_size(1920, 1080);
//! mosh.set_picture(Some(&clip_a));          // what gets moshed
//! if clip_b_has_new_frame {
//!     mosh.push_motion_frame(&clip_b);      // whose motion moves it
//! }
//! mosh.render(cx);                          // inside draw_walk
//! let out = mosh.output_texture();
//! ```

pub mod engine;
pub mod params;
pub mod transition;

use makepad_widgets::*;

pub use engine::{
    Datamosh, DatamoshView, DatamoshViewRef, DrawMoshHalve, DrawMoshIngest, DrawMoshLuma,
    DrawMoshMedian, DrawMoshOutput, DrawMoshRefine, DrawMoshSearch, DrawMoshStep, DrawMoshSubpel,
    DrawMoshVectors, LEVELS, SEARCH_RADIUS, SWEEPS,
};
pub use params::{MoshMode, MoshParams, MoshView, VectorFormat, VectorKind};
pub use transition::{TransitionFrame, TransitionMotion, TransitionParams, TransitionPhase};

/// Register the draw shaders and `DatamoshView`. Call after
/// `makepad_widgets::script_mod` and before any UI that names
/// `DatamoshView` or embeds a [`Datamosh`].
pub fn script_mod(vm: &mut ScriptVm) {
    crate::engine::script_mod(vm);
}
