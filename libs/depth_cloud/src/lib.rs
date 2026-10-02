//! `DepthCloud`: plays a video, or something already rendered, as a 3D
//! point cloud. From the front it reads as the flat picture; orbit the
//! camera and the pixels float apart by depth.
//!
//! * [`cloud`]: the widget (a child of any `XrSceneView`) and its shader:
//!   depth crop, flying-pixel edge cut, mouse effects.
//! * [`pipeline`]: the worker (decode, pacing, depth, stabilization).
//! * [`depth`]: depth sources: Depth-Anything-V2 / Video-Depth-Anything /
//!   DA3 (feature `localai`, CUDA), packed RGBD, depth-pass videos, priors.
//!
//! ```text
//! scene := XrSceneView{ cloud := DepthCloud{} }
//! ```
//! then `DepthCloud::open(cx, SourceSpec{...})` and read
//! [`DepthCloudAction`]s for status.

pub use makepad_widgets;
use makepad_widgets::ScriptVm;

pub mod cloud;
pub mod depth;
pub mod pipeline;
mod sim;

pub use cloud::{CloudEffect, DepthCloud, DepthCloudAction, RenderedDepth};
pub use depth::{DepthSource, FrameLayout};
pub use pipeline::{FrameStats, PipelineSettings, SourceSpec, VideoInput};

/// Registers the `DepthCloud` widget and its shader.
pub fn script_mod(vm: &mut ScriptVm) {
    cloud::script_mod(vm);
}
