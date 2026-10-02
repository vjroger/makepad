//! THE DECODER, entirely on the GPU: a reference picture that only ever
//! gets P-frames, fed with motion from whatever source the host likes.
//!
//! Per motion step (one new frame of the motion source):
//!
//! ```text
//!   motion source ──ingest──> RGB+luma history (2 frames, full size)
//!        │                         │
//!        │           luma pair at a quarter size, halved (LEVELS-1)x
//!        │           exhaustive block search at the top level
//!        │           per level: SWEEPS refinement sweeps, 3x3 median
//!        │           sub-pixel parabola  ──> estimated field (uv units)
//!        │
//!   or renderer vectors ──convert──> supplied field (uv units)
//!
//!   step:   reference'(x) = reference(x - v(block of x))   (+ residual,
//!           intra refresh, heal, codec damage)
//!   output: the reference (Decode) or the keyframe / live picture read
//!           through the moved coordinates (Remap), mixed with the dry
//!           picture
//! ```
//!
//! The estimator is the MPEG encoder's own job — find, for each block of
//! the new frame, the block of the previous frame it came from — done the
//! way `makepad-frametween` does its classical flow (pyramidal block
//! matching as ping-pong fragment passes; its constants are reused), but
//! reading GPU textures rather than uploaded NV12, so a live render can be
//! the motion source with no readback.
//!
//! Renderer laws (see `makepad-frametween`): every float data pass declares
//! its `color_format`; each stage opens its own root turtle; stages are
//! chained by parent links because sibling passes do not run in creation
//! order. Upstream producers that render a texture this engine reads in the
//! same frame register with [`Datamosh::depends_on`] so they run first.

use crate::params::{DriftMode, MoshMode, MoshParams, MoshView, VectorFormat, VectorKind};
use crate::transition::{TransitionFrame, TransitionMotion, TransitionParams, TransitionPhase};
use makepad_widgets::*;

/// Estimator pyramid levels. Level 0 is a quarter of the frame, so the top
/// cell spans 32 pixels and the top search (+-SEARCH_RADIUS cells) reaches
/// 128 pixels of motion per frame.
pub const LEVELS: usize = 4;
/// Refinement sweeps per pyramid level.
pub const SWEEPS: usize = 2;
/// Exhaustive search radius at the top level, in top-level cells.
pub const SEARCH_RADIUS: f32 = 4.0;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    // Every stage fills its own offscreen pass, so each shader transforms
    // in pure pass space (the stock DrawQuad vertex clips against the
    // parent window and would slice the pass).

    set_type_default() do #(DrawMoshIngest::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_src: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // RGB as it is, Rec.709 luma in alpha for the estimator.
        pixel: fn() {
            let c = self.tex_src.sample(self.pos)
            let l = c.x * 0.2126 + c.y * 0.7152 + c.z * 0.0722
            return vec4(c.x, c.y, c.z, l)
        }
    }

    set_type_default() do #(DrawMoshLuma::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_cur: texture_2d(float)
        tex_prev: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // A 4x4 box per cell from four bilinear taps, previous frame in R,
        // current in G, on the estimator's 0..255 luma scale.
        pixel: fn() {
            let o = self.inv_src
            let t00 = self.pos + vec2(0.0 - o.x, 0.0 - o.y)
            let t10 = self.pos + vec2(o.x, 0.0 - o.y)
            let t01 = self.pos + vec2(0.0 - o.x, o.y)
            let t11 = self.pos + vec2(o.x, o.y)
            let mut cur = self.tex_cur.sample(t00).w + self.tex_cur.sample(t10).w
            cur = cur + self.tex_cur.sample(t01).w + self.tex_cur.sample(t11).w
            let mut prev = self.tex_prev.sample(t00).w + self.tex_prev.sample(t10).w
            prev = prev + self.tex_prev.sample(t01).w + self.tex_prev.sample(t11).w
            return vec4(prev * 63.75, cur * 63.75, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshHalve::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_src: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // One centred bilinear tap is the exact 2x2 average.
        pixel: fn() {
            let s = self.tex_src.sample(self.pos)
            return vec4(s.x, s.y, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshSearch::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_luma: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // 5x5 mean absolute difference between the block of the CURRENT
        // frame here and the block of the PREVIOUS frame it came from, if
        // the content moved by d cells: the encoder's question.
        sad: fn(d: vec2) -> float {
            let mut sum = 0.0
            let mut j = -2.0
            loop {
                if j > 2.5 { break }
                let mut i = -2.0
                loop {
                    if i > 2.5 { break }
                    let at = self.pos + vec2(i, j) * self.inv_size
                    let back = self.pos + (vec2(i, j) - d) * self.inv_size
                    sum = sum + abs(self.tex_luma.sample_nearest(at).y - self.tex_luma.sample_nearest(back).x)
                    i = i + 1.0
                }
                j = j + 1.0
            }
            return sum * 0.04
        }
        // Full search at the top level: nothing to propagate yet. The tiny
        // magnitude bias breaks ties on flat regions toward zero motion.
        pixel: fn() {
            let mut best = vec2(0.0, 0.0)
            let mut best_cost = 1e30
            let mut dy = 0.0 - self.radius
            loop {
                if dy > self.radius + 0.5 { break }
                let mut dx = 0.0 - self.radius
                loop {
                    if dx > self.radius + 0.5 { break }
                    let c = self.sad(vec2(dx, dy)) + (abs(dx) + abs(dy)) * 0.003
                    if c < best_cost {
                        best_cost = c
                        best = vec2(dx, dy)
                    }
                    dx = dx + 1.0
                }
                dy = dy + 1.0
            }
            return vec4(best.x, best.y, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshRefine::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_luma: texture_2d(float)
        tex_prev: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // prev_scale carries a coarser level's vectors (in its cells) into
        // this level's cells.
        prev_at: fn(uv: vec2) -> vec2 {
            let s = self.tex_prev.sample_nearest(uv)
            return vec2(s.x, s.y) * self.prev_scale
        }
        sad: fn(d: vec2) -> float {
            let mut sum = 0.0
            let mut j = -2.0
            loop {
                if j > 2.5 { break }
                let mut i = -2.0
                loop {
                    if i > 2.5 { break }
                    let at = self.pos + vec2(i, j) * self.inv_size
                    let back = self.pos + (vec2(i, j) - d) * self.inv_size
                    sum = sum + abs(self.tex_luma.sample_nearest(at).y - self.tex_luma.sample_nearest(back).x)
                    i = i + 1.0
                }
                j = j + 1.0
            }
            return sum * 0.04
        }
        // Smoothness is charged less across luma edges, so a moving object
        // does not drag its background along.
        edge_weight: fn() -> float {
            let l = self.tex_luma.sample_nearest(self.pos - vec2(self.inv_size.x, 0.0)).y
            let r = self.tex_luma.sample_nearest(self.pos + vec2(self.inv_size.x, 0.0)).y
            let u = self.tex_luma.sample_nearest(self.pos - vec2(0.0, self.inv_size.y)).y
            let dn = self.tex_luma.sample_nearest(self.pos + vec2(0.0, self.inv_size.y)).y
            return 1.0 / (1.0 + (abs(r - l) + abs(dn - u)) * 0.06)
        }
        smooth: fn(d: vec2) -> float {
            let l = self.prev_at(self.pos + vec2(0.0 - self.inv_size.x, 0.0))
            let r = self.prev_at(self.pos + vec2(self.inv_size.x, 0.0))
            let u = self.prev_at(self.pos + vec2(0.0, 0.0 - self.inv_size.y))
            let dn = self.prev_at(self.pos + vec2(0.0, self.inv_size.y))
            let mut sum = abs(d.x - l.x) + abs(d.y - l.y)
            sum = sum + abs(d.x - r.x) + abs(d.y - r.y)
            sum = sum + abs(d.x - u.x) + abs(d.y - u.y)
            sum = sum + abs(d.x - dn.x) + abs(d.y - dn.y)
            return sum * 0.25
        }
        cost: fn(d: vec2, ew: float) -> float {
            return self.sad(d) + self.lambda * ew * self.smooth(d)
        }
        pixel: fn() {
            let ew = self.edge_weight()
            let here = self.prev_at(self.pos)
            let mut best = here
            let mut best_cost = self.cost(here, ew)
            // Neighbour propagation: a good vector crosses flat patches.
            let mut k = 0.0
            loop {
                if k > 3.5 { break }
                let mut off = vec2(0.0 - self.inv_size.x, 0.0)
                if k > 0.5 { off = vec2(self.inv_size.x, 0.0) }
                if k > 1.5 { off = vec2(0.0, 0.0 - self.inv_size.y) }
                if k > 2.5 { off = vec2(0.0, self.inv_size.y) }
                let cand = self.prev_at(self.pos + off)
                let c = self.cost(cand, ew)
                if c < best_cost {
                    best_cost = c
                    best = cand
                }
                k = k + 1.0
            }
            // Local refinement around the incumbent.
            let mut n = 0.0
            loop {
                if n > 11.5 { break }
                let mut o = vec2(-1.0, 0.0)
                if n > 0.5 { o = vec2(1.0, 0.0) }
                if n > 1.5 { o = vec2(0.0, -1.0) }
                if n > 2.5 { o = vec2(0.0, 1.0) }
                if n > 3.5 { o = vec2(-1.0, -1.0) }
                if n > 4.5 { o = vec2(1.0, -1.0) }
                if n > 5.5 { o = vec2(-1.0, 1.0) }
                if n > 6.5 { o = vec2(1.0, 1.0) }
                if n > 7.5 { o = vec2(-2.0, 0.0) }
                if n > 8.5 { o = vec2(2.0, 0.0) }
                if n > 9.5 { o = vec2(0.0, -2.0) }
                if n > 10.5 { o = vec2(0.0, 2.0) }
                let cand = here + o
                let c = self.cost(cand, ew)
                if c < best_cost {
                    best_cost = c
                    best = cand
                }
                n = n + 1.0
            }
            return vec4(best.x, best.y, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshMedian::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_src: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // Exact 3x3 median per component (Smith's network).
        med3: fn(a: float, b: float, c: float) -> float {
            return max(min(a, b), min(max(a, b), c))
        }
        med9: fn(a: float, b: float, c: float, d: float, e: float, f: float, g: float, h: float, i: float) -> float {
            let lo = max(max(min(min(a, b), c), min(min(d, e), f)), min(min(g, h), i))
            let mid = self.med3(self.med3(a, b, c), self.med3(d, e, f), self.med3(g, h, i))
            let hi = min(min(max(max(a, b), c), max(max(d, e), f)), max(max(g, h), i))
            return self.med3(lo, mid, hi)
        }
        pixel: fn() {
            let dx = self.inv_size.x
            let dy = self.inv_size.y
            let s00 = self.tex_src.sample_nearest(self.pos + vec2(0.0 - dx, 0.0 - dy))
            let s10 = self.tex_src.sample_nearest(self.pos + vec2(0.0, 0.0 - dy))
            let s20 = self.tex_src.sample_nearest(self.pos + vec2(dx, 0.0 - dy))
            let s01 = self.tex_src.sample_nearest(self.pos + vec2(0.0 - dx, 0.0))
            let s11 = self.tex_src.sample_nearest(self.pos)
            let s21 = self.tex_src.sample_nearest(self.pos + vec2(dx, 0.0))
            let s02 = self.tex_src.sample_nearest(self.pos + vec2(0.0 - dx, dy))
            let s12 = self.tex_src.sample_nearest(self.pos + vec2(0.0, dy))
            let s22 = self.tex_src.sample_nearest(self.pos + vec2(dx, dy))
            let mx = self.med9(s00.x, s10.x, s20.x, s01.x, s11.x, s21.x, s02.x, s12.x, s22.x)
            let my = self.med9(s00.y, s10.y, s20.y, s01.y, s11.y, s21.y, s02.y, s12.y, s22.y)
            return vec4(mx, my, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshSubpel::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_luma: texture_2d(float)
        tex_field: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        sad: fn(d: vec2) -> float {
            let mut sum = 0.0
            let mut j = -2.0
            loop {
                if j > 2.5 { break }
                let mut i = -2.0
                loop {
                    if i > 2.5 { break }
                    let at = self.pos + vec2(i, j) * self.inv_size
                    let back = self.pos + (vec2(i, j) - d) * self.inv_size
                    sum = sum + abs(self.tex_luma.sample_nearest(at).y - self.tex_luma.sample_nearest(back).x)
                    i = i + 1.0
                }
                j = j + 1.0
            }
            return sum * 0.04
        }
        // Parabola fit on the cost around the integer optimum, per axis.
        // Out: the final field in uv units, and the match cost (0..1 luma)
        // in z: a poor match is new content, which the step treats as
        // codec damage.
        pixel: fn() {
            let s = self.tex_field.sample_nearest(self.pos)
            let d = vec2(s.x, s.y)
            let c0 = self.sad(d)
            let cl = self.sad(d + vec2(-1.0, 0.0))
            let cr = self.sad(d + vec2(1.0, 0.0))
            let cu = self.sad(d + vec2(0.0, -1.0))
            let cd = self.sad(d + vec2(0.0, 1.0))
            let mut fx = 0.0
            let dxx = cl + cr - 2.0 * c0
            if dxx > 1e-6 {
                fx = clamp(0.5 * (cl - cr) / dxx, -0.5, 0.5)
            }
            let mut fy = 0.0
            let dyy = cu + cd - 2.0 * c0
            if dyy > 1e-6 {
                fy = clamp(0.5 * (cu - cd) / dyy, -0.5, 0.5)
            }
            return vec4((d.x + fx) * self.inv_size.x, (d.y + fy) * self.inv_size.y, c0 / 255.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshVectors::script_shader(vm)){
        ..mod.draw.DrawQuad
        color_format: @Rgba16F
        tex_vec: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // A renderer's own motion output, normalised to the field the
        // step reads: forward motion in top-left uv units.
        pixel: fn() {
            let s = self.tex_vec.sample_nearest(self.pos)
            let raw = vec2(s.x, s.y) * self.vec_scale
            let prev = vec2(raw.x, mix(raw.y, 1.0 - raw.y, self.flip_y))
            let delta = vec2(raw.x, mix(raw.y, 0.0 - raw.y, self.flip_y)) * self.vec_sign
            let d = mix(delta, self.pos - prev, self.absolute)
            return vec4(d.x, d.y, 0.0, 1.0)
        }
    }

    set_type_default() do #(DrawMoshStep::script_shader(vm)){
        ..mod.draw.DrawQuad
        // The reference is 32-bit float: Remap keeps COORDINATES in it,
        // and half floats would quantise them to about a pixel. Every read
        // is nearest (32-bit float is not filterable everywhere); Decode's
        // bilinear is done by hand.
        color_format: @Rgba32F
        tex_ref: texture_2d(float)
        tex_field: texture_2d(float)
        tex_picture: texture_2d(float)
        tex_mcur: texture_2d(float)
        tex_mprev: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        // Hash without sine (Hoskins): stable across GPUs.
        rand: fn(p: vec2) -> float {
            let p3 = fract(vec3(p.x, p.y, p.x) * 0.1031)
            let k = dot(p3, vec3(p3.y, p3.z, p3.x) + vec3(33.33, 33.33, 33.33))
            let q = p3 + vec3(k, k, k)
            return fract((q.x + q.y) * q.z)
        }
        // The drift pattern at a block centre, in pixels. The radial ones
        // are measured from the frame centre and reach `drift` at the
        // nearest frame edge.
        drift_at: fn(c: vec2) -> vec2 {
            let a = self.drift
            let rel = (c - vec2(0.5, 0.5)) * self.frame_size
            let r = a / (0.5 * min(self.frame_size.x, self.frame_size.y))
            let turn = vec2(0.0 - rel.y, rel.x) * r
            let outward = rel * r
            if self.drift_mode > 3.5 {
                return (turn + outward) * 0.7071
            }
            if self.drift_mode > 2.5 {
                return outward
            }
            if self.drift_mode > 1.5 {
                return turn
            }
            if self.drift_mode > 0.5 {
                return vec2(0.0, a)
            }
            return vec2(a, 0.0)
        }
        ref_bilinear: fn(uv: vec2) -> vec4 {
            let p = uv * self.frame_size - vec2(0.5, 0.5)
            let f = fract(p)
            let b = (floor(p) + vec2(0.5, 0.5)) * self.inv_frame
            let s00 = self.tex_ref.sample_nearest(b)
            let s10 = self.tex_ref.sample_nearest(b + vec2(self.inv_frame.x, 0.0))
            let s01 = self.tex_ref.sample_nearest(b + vec2(0.0, self.inv_frame.y))
            let s11 = self.tex_ref.sample_nearest(b + self.inv_frame)
            return mix(mix(s00, s10, f.x), mix(s01, s11, f.x), f.y)
        }
        pixel: fn() {
            let px = self.pos * self.frame_size
            let bs = max(self.block, 1.0)
            let cell = floor(px / bs)
            let fresh = self.tex_picture.sample(self.pos).xyz
            // I-FRAME: the reference becomes the picture (Decode) or the
            // identity mapping onto the keyframe just copied (Remap).
            if self.keyframe > 0.5 {
                if self.remap > 0.5 {
                    return vec4(self.pos.x, self.pos.y, 0.0, 1.0)
                }
                return vec4(fresh.x, fresh.y, fresh.z, 0.0)
            }
            // One vector per macroblock, read at the block's centre.
            let center = (cell + vec2(0.5, 0.5)) * bs * self.inv_frame
            let f = self.tex_field.sample(mix(self.pos, center, step(1.5, bs)))
            let raw = vec2(f.x, f.y) * self.field_on
            let cost = f.z * self.field_on
            let seed = self.seed
            let r1 = self.rand(cell + vec2(seed * 1.37 + 0.11, 17.0))
            let r2 = self.rand(cell + vec2(29.0, seed * 2.11 + 0.37))
            let r3 = self.rand(cell + vec2(seed * 0.73 + 41.0, seed * 1.91 + 5.0))
            let r4 = self.rand(cell * 1.7 + vec2(seed * 3.17 + 3.0, 9.0))
            let r5 = self.rand(cell * 0.61 + vec2(53.0, seed * 0.57 + 13.0))
            let mm = self.motion_mat
            let mut m = vec2(raw.x * mm.x + raw.y * mm.y, raw.x * mm.z + raw.y * mm.w) * self.gain
            m = m + self.drift_at(center) * self.inv_frame
            m = m + (vec2(r1, r2) - vec2(0.5, 0.5)) * self.diffusion * self.inv_frame
            // The decoder's sub-pixel precision.
            if self.pel > 0.5 {
                let q = self.frame_size * self.pel
                m = round(m * q) / q
            }
            let src = self.pos - m
            let move_px = length(m * self.frame_size)
            // Intra refresh: this block is sent again from the picture.
            let refreshed = 1.0 - step(self.refresh, r3)
            let damage_in = clamp(move_px / bs, 0.0, 1.0) * 0.06 + cost * 0.5
            if self.remap > 0.5 {
                // Nearest texel of the coordinate map plus the sub-texel
                // remainder: exact for a translation, and never blends two
                // unrelated coordinates across a block edge.
                let t = (floor(src * self.frame_size) + vec2(0.5, 0.5)) * self.inv_frame
                let prev = self.tex_ref.sample_nearest(t)
                let inside = step(0.0, src.x) * step(src.x, 1.0) * step(0.0, src.y) * step(src.y, 1.0)
                // Uncovered from outside the frame: there is no history,
                // so the block shows what the keyframe has there.
                let mut uv = mix(self.pos, vec2(prev.x, prev.y) + (src - t), inside)
                let damage = min(prev.z * 0.985 + damage_in, 4.0) * inside
                // Codec damage in coordinate space: a broken block lands
                // displaced by up to half a block.
                if r4 < self.entropy * smoothstep(0.1, 1.5, damage) * 0.25 {
                    uv = uv + (vec2(r1, r5) - vec2(0.5, 0.5)) * bs * self.inv_frame
                }
                uv = mix(uv, self.pos, self.heal)
                uv = mix(uv, self.pos, refreshed)
                return vec4(uv.x, uv.y, damage * (1.0 - refreshed), 1.0)
            }
            let prev = self.ref_bilinear(src)
            let mut col = vec3(prev.x, prev.y, prev.z)
            // The P-frame residual as the motion source's encoder would
            // have coded it: against ITS previous frame, along the
            // unmodified block vector.
            let res = self.tex_mcur.sample(self.pos).xyz - self.tex_mprev.sample(self.pos - raw).xyz
            col = col + res * self.residual
            let damage = min(prev.w * 0.985 + damage_in, 4.0)
            // Codec damage in pixel space: one DCT basis pattern stamped
            // on the block, the ringing and block breakup of a starved
            // encoder.
            if r4 < self.entropy * smoothstep(0.1, 1.5, damage) {
                let local = px - cell * bs
                let fu = floor(r1 * 4.0)
                let fv = floor(r2 * 4.0)
                let basis = cos(3.14159265 * (local.x + 0.5) * fu / bs) * cos(3.14159265 * (local.y + 0.5) * fv / bs)
                let amp = (r5 - 0.5) * 0.6 * self.entropy
                col = col + vec3(1.0, 1.0 - r2 * 0.3, 1.0 - r1 * 0.3) * (basis * amp)
            }
            col = clamp(col, vec3(-0.25, -0.25, -0.25), vec3(1.25, 1.25, 1.25))
            col = mix(col, fresh, self.heal)
            col = mix(col, fresh, refreshed)
            let keep = (1.0 - refreshed) * (1.0 - self.heal)
            return vec4(col.x, col.y, col.z, damage * keep)
        }
    }

    set_type_default() do #(DrawMoshOutput::script_shader(vm)){
        ..mod.draw.DrawQuad
        tex_ref: texture_2d(float)
        tex_picture: texture_2d(float)
        tex_key: texture_2d(float)
        tex_field: texture_2d(float)
        vertex: fn() {
            let clipped = self.geom.pos * self.rect_size + self.rect_pos
            self.pos = self.geom.pos
            self.world = vec4(clipped.x, clipped.y, self.draw_depth, 1.0)
            return self.draw_pass.camera_projection * (self.draw_pass.camera_view * self.world)
        }
        hue: fn(h: float) -> vec3 {
            let r = abs(h * 6.0 - 3.0) - 1.0
            let g = 2.0 - abs(h * 6.0 - 2.0)
            let b = 2.0 - abs(h * 6.0 - 4.0)
            return clamp(vec3(r, g, b), vec3(0.0, 0.0, 0.0), vec3(1.0, 1.0, 1.0))
        }
        // The same drift pattern as the step, so the blur follows exactly
        // the push the step applied.
        drift_at: fn(c: vec2) -> vec2 {
            let a = self.drift
            let rel = (c - vec2(0.5, 0.5)) * self.frame_size
            let r = a / (0.5 * min(self.frame_size.x, self.frame_size.y))
            let turn = vec2(0.0 - rel.y, rel.x) * r
            let outward = rel * r
            if self.drift_mode > 3.5 {
                return (turn + outward) * 0.7071
            }
            if self.drift_mode > 2.5 {
                return outward
            }
            if self.drift_mode > 1.5 {
                return turn
            }
            if self.drift_mode > 0.5 {
                return vec2(0.0, a)
            }
            return vec2(a, 0.0)
        }
        // The moshed colour at uv, before the dry mix.
        mosh_at: fn(uv: vec2) -> vec3 {
            let r = self.tex_ref.sample_nearest(uv)
            let at = vec2(r.x, r.y)
            let remapped = mix(self.tex_key.sample(at).xyz, self.tex_picture.sample(at).xyz, self.live_on)
            return mix(vec3(r.x, r.y, r.z), remapped, self.remap)
        }
        pixel: fn() {
            let r = self.tex_ref.sample_nearest(self.pos)
            let pic = self.tex_picture.sample(self.pos).xyz
            if self.view_mode > 1.5 {
                let k = clamp(mix(r.w, r.z, self.remap) * 0.5, 0.0, 1.0)
                return vec4(k, k * k, k * k * k, 1.0)
            }
            if self.view_mode > 0.5 {
                let v = self.tex_field.sample(self.pos).xy * self.frame_size
                let speed = clamp(length(v) * 0.08, 0.0, 1.0)
                let c = self.hue(atan2(v.y, v.x) / 6.2831853 + 0.5) * speed
                return vec4(c.x, c.y, c.z, 1.0)
            }
            let mut mosh = self.mosh_at(self.pos)
            // MOTION BLUR, on the output only (the reference stays sharp, so
            // it never accumulates): a streak along this block's motion
            // vector and/or its drift, `blur_*` steps long.
            if self.blur_motion + self.blur_drift > 0.001 {
                let bs = max(self.block, 1.0)
                let cell = floor(self.pos * self.frame_size / bs)
                let center = mix(self.pos, (cell + vec2(0.5, 0.5)) * bs * self.inv_frame, step(1.5, bs))
                let f = self.tex_field.sample(center)
                let raw = vec2(f.x, f.y) * self.field_on
                let mm = self.motion_mat
                let mv = vec2(raw.x * mm.x + raw.y * mm.y, raw.x * mm.z + raw.y * mm.w) * self.gain
                let streak = mv * self.blur_motion + self.drift_at(center) * self.inv_frame * self.blur_drift
                let mut sum = vec3(0.0, 0.0, 0.0)
                let mut k = 0.0
                loop {
                    if k > 11.5 { break }
                    sum = sum + self.mosh_at(self.pos - streak * (k / 11.0 - 0.5))
                    k = k + 1.0
                }
                mosh = sum / 12.0
            }
            let c = mix(pic, mosh, self.wet)
            return vec4(clamp(c.x, 0.0, 1.0), clamp(c.y, 0.0, 1.0), clamp(c.z, 0.0, 1.0), 1.0)
        }
    }

    mod.widgets.DatamoshViewBase = #(DatamoshView::register_widget(vm))
    mod.widgets.DatamoshView = set_type_default() do mod.widgets.DatamoshViewBase{
        width: 4
        height: 4
    }
}

// Per the draw-shader layout law: only `#[live]` instance fields after the
// `#[deref]`.

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshIngest {
    #[deref]
    pub draw_super: DrawQuad,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshLuma {
    #[deref]
    pub draw_super: DrawQuad,
    /// One texel of the full-size history in uv units.
    #[live]
    pub inv_src: Vec2f,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshHalve {
    #[deref]
    pub draw_super: DrawQuad,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshSearch {
    #[deref]
    pub draw_super: DrawQuad,
    /// One cell of this level in uv units.
    #[live]
    pub inv_size: Vec2f,
    #[live]
    pub radius: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshRefine {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub inv_size: Vec2f,
    /// This level's cells per cell of the level `tex_prev` was solved on.
    #[live]
    pub prev_scale: Vec2f,
    #[live(1.5)]
    pub lambda: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshMedian {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub inv_size: Vec2f,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshSubpel {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub inv_size: Vec2f,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshVectors {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub vec_scale: Vec2f,
    /// +1 forward vectors, -1 backward.
    #[live]
    pub vec_sign: f32,
    /// 1 when RG is the previous position rather than a delta.
    #[live]
    pub absolute: f32,
    #[live]
    pub flip_y: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshStep {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub frame_size: Vec2f,
    #[live]
    pub inv_frame: Vec2f,
    #[live]
    pub block: f32,
    #[live]
    pub gain: f32,
    /// Row-major 2x2 applied to every vector.
    #[live]
    pub motion_mat: Vec4f,
    /// Pixels per step (see `drift_at`).
    #[live]
    pub drift: f32,
    #[live]
    pub drift_mode: f32,
    #[live]
    pub diffusion: f32,
    #[live]
    pub pel: f32,
    #[live]
    pub refresh: f32,
    #[live]
    pub heal: f32,
    /// 0 when this step has no fresh motion-source pair.
    #[live]
    pub residual: f32,
    #[live]
    pub entropy: f32,
    #[live]
    pub seed: f32,
    #[live]
    pub remap: f32,
    #[live]
    pub keyframe: f32,
    #[live]
    pub field_on: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawMoshOutput {
    #[deref]
    pub draw_super: DrawQuad,
    #[live]
    pub frame_size: Vec2f,
    #[live]
    pub remap: f32,
    #[live]
    pub live_on: f32,
    #[live]
    pub wet: f32,
    /// 0 output, 1 vectors, 2 damage.
    #[live]
    pub view_mode: f32,
    #[live]
    pub inv_frame: Vec2f,
    #[live]
    pub block: f32,
    #[live]
    pub gain: f32,
    #[live]
    pub motion_mat: Vec4f,
    #[live]
    pub drift: f32,
    #[live]
    pub drift_mode: f32,
    #[live]
    pub field_on: f32,
    /// Streak length along the motion vectors, in steps.
    #[live]
    pub blur_motion: f32,
    /// Streak length along the drift, in steps.
    #[live]
    pub blur_drift: f32,
}

/// One offscreen stage: its pass and its draw list.
struct Stage {
    pass: DrawPass,
    draw_list: DrawList,
}

/// Everything sized by the frame.
struct Targets {
    /// Motion-source history, RGB + luma: one slot is the latest frame,
    /// the other the one before it.
    motion: [Texture; 2],
    /// Estimator luma pyramid (previous in R, current in G).
    luma: Vec<Texture>,
    /// Estimator ping, pong, and per-level median output.
    scratch: [Texture; 3],
    /// The estimated field (uv units, match cost in z), a quarter size.
    field_est: Texture,
    /// The supplied field (uv units), full size.
    field_sup: Texture,
    /// The decoder's reference, double buffered.
    reference: [Texture; 2],
    /// Remap's keyframe.
    key: Texture,
    output: Texture,
    /// Bound in any slot that has nothing to show yet.
    blank: Texture,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum FieldSource {
    #[default]
    None,
    Estimated,
    Supplied,
}

enum MotionInput {
    Frame(Texture),
    Vectors {
        vectors: Texture,
        format: VectorFormat,
        color: Option<Texture>,
    },
}

enum Op {
    Ingest { src: Texture, slot: usize },
    Luma { cur: usize },
    Halve { level: usize },
    Search,
    Refine { level: usize, sweep: usize },
    Median { level: usize },
    Subpel,
    Vectors { src: Texture, format: VectorFormat },
    KeyCopy,
    Step { keyframe: bool, residual: f32 },
    Output,
}

/// The datamosh engine as an embeddable component: a host widget keeps one
/// as a `#[live]` field, feeds it from its own event and draw code, calls
/// [`Datamosh::render`] from its `draw_walk` and samples
/// [`Datamosh::output_texture`]. [`DatamoshView`] is the same thing as a
/// widget for a Splash tree.
///
/// Feeding it, per display frame, any of:
///
/// - [`Datamosh::set_picture`]: what intra-refreshed blocks, keyframes and
///   the dry mix show;
/// - [`Datamosh::push_motion_frame`]: the motion source advanced; estimate
///   its motion and decode one step with it;
/// - [`Datamosh::push_motion_vectors`]: the same with a renderer's own
///   vectors;
/// - [`Datamosh::repeat_step`]: decode one more step with the last field
///   (the duplicated-P-frame "bloom");
/// - [`Datamosh::keyframe`]: an I-frame — the reference becomes the
///   picture;
/// - [`Datamosh::drive_transition`]: all of the above, scheduled by a
///   transition's progress.
///
/// Steps happen only when asked for, so the moshing runs at the motion
/// source's frame rate, not the display's.
#[derive(Script, ScriptHook)]
pub struct Datamosh {
    #[live]
    draw_ingest: DrawMoshIngest,
    #[live]
    draw_luma: DrawMoshLuma,
    #[live]
    draw_halve: DrawMoshHalve,
    #[live]
    draw_search: DrawMoshSearch,
    #[live]
    draw_refine: DrawMoshRefine,
    #[live]
    draw_median: DrawMoshMedian,
    #[live]
    draw_subpel: DrawMoshSubpel,
    #[live]
    draw_vectors: DrawMoshVectors,
    #[live]
    draw_step: DrawMoshStep,
    #[live]
    draw_output: DrawMoshOutput,
    #[rust]
    params: MoshParams,
    #[rust]
    frame_size: (usize, usize),
    #[rust]
    targets: Option<Targets>,
    #[rust]
    stages: Vec<Stage>,
    #[rust]
    picture: Option<Texture>,
    #[rust]
    pending_motion: Option<MotionInput>,
    #[rust]
    pending_step: bool,
    #[rust]
    pending_keyframe: bool,
    /// Which history slot holds the latest motion frame.
    #[rust]
    latest_motion: usize,
    /// Motion frames ingested since the last reset (saturating).
    #[rust]
    motion_frames: u32,
    #[rust]
    field: FieldSource,
    /// Which reference buffer is current.
    #[rust]
    front: usize,
    /// Whether the reference holds coordinates (Remap) or colours, as of
    /// the last keyframe; None until the first one. A mode change across
    /// that line forces a keyframe.
    #[rust]
    keyed_remap: Option<bool>,
    #[rust]
    steps: u32,
    /// The running transition's plan, when one is driving the decoder.
    #[rust]
    transition: Option<TransitionFrame>,
    /// Passes that render textures this frame reads; linked under the
    /// first stage so they run before it.
    #[rust]
    upstream: Vec<DrawPassId>,
    #[rust]
    rendered: bool,
    /// What [`DriftMode::Random`] stands for until the next keyframe.
    #[rust]
    random_drift: Option<DriftMode>,
    #[rust]
    rng: u32,
}

impl Datamosh {
    /// The output size. Every pixel distance in [`MoshParams`] is in this
    /// frame's pixels. Changing it drops all state and starts over from a
    /// keyframe.
    pub fn set_frame_size(&mut self, width: usize, height: usize) {
        if self.frame_size == (width, height) {
            return;
        }
        self.frame_size = (width, height);
        self.targets = None;
        self.rendered = false;
        self.reset();
    }

    pub fn frame_size(&self) -> (usize, usize) {
        self.frame_size
    }

    /// Forget the motion history and decode from a fresh keyframe.
    pub fn reset(&mut self) {
        self.motion_frames = 0;
        self.field = FieldSource::None;
        self.keyed_remap = None;
        self.pending_motion = None;
        self.pending_step = false;
    }

    pub fn params(&self) -> &MoshParams {
        &self.params
    }

    pub fn set_params(&mut self, params: MoshParams) {
        self.params = params;
    }

    /// The picture: what keyframes, refreshed blocks, healing and the dry
    /// mix show. Without one the motion source's own latest frame is used,
    /// which moshes a clip with its own motion.
    pub fn set_picture(&mut self, picture: Option<&Texture>) {
        self.picture = picture.cloned();
    }

    /// The motion source has a new frame: estimate how it moved since its
    /// previous one and decode one step with that motion. The texture is
    /// read when the engine renders; a host that renders it in the same
    /// frame registers its pass with [`Datamosh::depends_on`].
    pub fn push_motion_frame(&mut self, frame: &Texture) {
        self.pending_motion = Some(MotionInput::Frame(frame.clone()));
        self.pending_step = true;
    }

    /// Decode one step with a renderer's own motion vectors (a float
    /// texture). `color` is that render's picture: with it the step can add
    /// the render's residual, without it the vectors move the reference
    /// alone.
    pub fn push_motion_vectors(
        &mut self,
        vectors: &Texture,
        format: VectorFormat,
        color: Option<&Texture>,
    ) {
        self.pending_motion = Some(MotionInput::Vectors {
            vectors: vectors.clone(),
            format,
            color: color.cloned(),
        });
        self.pending_step = true;
    }

    /// Decode one more step with the last motion field and no residual:
    /// the same P-frame duplicated, which makes moving blocks bloom and
    /// slide on.
    pub fn repeat_step(&mut self) {
        self.pending_step = true;
    }

    /// An I-frame on the next render: the reference becomes the picture.
    /// Takes precedence over a step queued for the same render; motion
    /// pushed for it still updates the history.
    pub fn keyframe(&mut self) {
        self.pending_keyframe = true;
    }

    /// Schedule one display frame of a datamosh transition from `from` to
    /// `to` at `progress` (0 = the cut, 1 = done). `advanced` is whether the
    /// clip whose motion drives the mosh has a new frame this display
    /// frame; call this every display frame of the transition, before and
    /// after it too, so the motion history is warm when the cut comes.
    ///
    /// While it runs the decoder is in [`MoshMode::Decode`] with the plan's
    /// refresh, heal and residual; the rest of [`MoshParams`] (block size,
    /// gain, entropy, ...) shapes the mosh as usual.
    pub fn drive_transition(
        &mut self,
        from: &Texture,
        to: &Texture,
        progress: f32,
        transition: &TransitionParams,
        advanced: bool,
    ) -> TransitionPhase {
        let frame = transition.frame(progress);
        let motion = match transition.motion {
            TransitionMotion::Incoming => to,
            TransitionMotion::Outgoing => from,
        };
        if advanced {
            self.push_motion_frame(motion);
        }
        // The whole transition decodes colours, before the cut included:
        // the reference must already hold the outgoing picture when the
        // first P-frame lands on it.
        self.transition = Some(frame);
        match frame.phase {
            TransitionPhase::Before => {
                self.set_picture(Some(from));
                self.keyframe();
            }
            TransitionPhase::Mosh => self.set_picture(Some(to)),
            TransitionPhase::After => {
                self.set_picture(Some(to));
                self.keyframe();
            }
        }
        frame.phase
    }

    /// Hand the decoder back to the plain [`MoshParams`] after a
    /// transition (which keeps it in [`MoshMode::Decode`] for as long as it
    /// is being driven, `After` included).
    pub fn end_transition(&mut self) {
        self.transition = None;
    }

    /// A pass that renders a texture this engine reads in the same frame
    /// (the picture, a motion frame, vectors). Declare it on every frame it
    /// renders; it is linked under the engine's first stage so it runs
    /// before the engine samples it.
    pub fn depends_on(&mut self, pass: &DrawPass) {
        self.upstream.push(pass.draw_pass_id());
    }

    /// The moshed picture (BGRA8, frame size), once rendered.
    pub fn output_texture(&self) -> Option<Texture> {
        match (&self.targets, self.rendered) {
            (Some(targets), true) => Some(targets.output.clone()),
            _ => None,
        }
    }

    /// The motion field the decoder currently uses (forward motion in uv
    /// units in RG; estimated fields carry the match cost in B).
    pub fn field_texture(&self) -> Option<Texture> {
        let targets = self.targets.as_ref()?;
        match self.field {
            FieldSource::None => None,
            FieldSource::Estimated => Some(targets.field_est.clone()),
            FieldSource::Supplied => Some(targets.field_sup.clone()),
        }
    }

    /// Level-0 grid of the estimator: a cell per 4x4 pixels.
    fn grid(&self) -> (usize, usize) {
        (
            (self.frame_size.0 / 4).max(8),
            (self.frame_size.1 / 4).max(8),
        )
    }

    fn level_dims(&self, level: usize) -> (usize, usize) {
        let (gw, gh) = self.grid();
        ((gw >> level).max(8), (gh >> level).max(8))
    }

    fn ensure_targets(&mut self, cx: &mut Cx) {
        if self.targets.is_some() {
            return;
        }
        let (w, h) = self.frame_size;
        let fixed = TextureSize::Fixed {
            width: w,
            height: h,
        };
        let half_fixed = |cx: &mut Cx| {
            Texture::new_with_format(
                cx,
                TextureFormat::RenderRGBAf16 {
                    size: fixed.clone(),
                    initial: true,
                },
            )
        };
        let half_auto = |cx: &mut Cx| {
            Texture::new_with_format(
                cx,
                TextureFormat::RenderRGBAf16 {
                    size: TextureSize::Auto,
                    initial: true,
                },
            )
        };
        let full_fixed = |cx: &mut Cx| {
            Texture::new_with_format(
                cx,
                TextureFormat::RenderRGBAf32 {
                    size: fixed.clone(),
                    initial: true,
                },
            )
        };
        self.targets = Some(Targets {
            motion: [half_fixed(cx), half_fixed(cx)],
            luma: (0..LEVELS).map(|_| half_auto(cx)).collect(),
            scratch: [half_auto(cx), half_auto(cx), half_auto(cx)],
            field_est: half_auto(cx),
            field_sup: half_fixed(cx),
            reference: [full_fixed(cx), full_fixed(cx)],
            key: half_fixed(cx),
            output: Texture::new_with_format(
                cx,
                TextureFormat::RenderBGRAu8 {
                    size: fixed,
                    initial: true,
                },
            ),
            blank: Texture::new_with_format(
                cx,
                TextureFormat::VecBGRAu8_32 {
                    width: 1,
                    height: 1,
                    data: Some(vec![0xff00_0000]),
                    updated: TextureUpdated::Full,
                },
            ),
        });
        self.front = 0;
        self.latest_motion = 0;
    }

    /// The concrete pattern for `mode`: Random is rolled once per keyframe
    /// and held until the next one.
    fn drift_pick(&mut self, mode: DriftMode) -> DriftMode {
        if mode != DriftMode::Random {
            return mode;
        }
        if let Some(pick) = self.random_drift {
            return pick;
        }
        // xorshift32, seeded off the step counter on first use.
        let mut x = self.rng ^ self.steps.wrapping_mul(0x9e37_79b9) ^ 0x2545_f491;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        let pick = DriftMode::CONCRETE[x as usize % DriftMode::CONCRETE.len()];
        self.random_drift = Some(pick);
        pick
    }

    /// The parameters this render decodes with: the transition's plan, if
    /// one is running, over the plain ones.
    fn effective_params(&self) -> MoshParams {
        let mut params = self.params;
        if let Some(frame) = self.transition {
            params.mode = MoshMode::Decode;
            if frame.phase == TransitionPhase::Mosh {
                params.refresh = frame.refresh;
                params.heal = frame.heal;
                params.residual = frame.residual;
                params.wet *= frame.wet;
            }
        }
        params
    }

    /// Turn what was queued since the last render into this render's pass
    /// list, advancing the bookkeeping the ops will make true.
    fn plan(&mut self, params: &MoshParams) -> Vec<Op> {
        let mut ops = Vec::new();
        let mut fresh_pair = false;
        match self.pending_motion.take() {
            Some(MotionInput::Frame(src)) => {
                let slot = self.latest_motion ^ 1;
                ops.push(Op::Ingest { src, slot });
                self.latest_motion = slot;
                self.motion_frames = self.motion_frames.saturating_add(1);
                if self.motion_frames >= 2 {
                    ops.push(Op::Luma { cur: slot });
                    for level in 1..LEVELS {
                        ops.push(Op::Halve { level });
                    }
                    ops.push(Op::Search);
                    for level in (0..LEVELS).rev() {
                        for sweep in 0..SWEEPS {
                            ops.push(Op::Refine { level, sweep });
                        }
                        ops.push(Op::Median { level });
                    }
                    ops.push(Op::Subpel);
                    self.field = FieldSource::Estimated;
                    fresh_pair = true;
                }
            }
            Some(MotionInput::Vectors {
                vectors,
                format,
                color,
            }) => {
                ops.push(Op::Vectors {
                    src: vectors,
                    format,
                });
                self.field = FieldSource::Supplied;
                if let Some(src) = color {
                    let slot = self.latest_motion ^ 1;
                    ops.push(Op::Ingest { src, slot });
                    self.latest_motion = slot;
                    self.motion_frames = self.motion_frames.saturating_add(1);
                    fresh_pair = self.motion_frames >= 2;
                }
            }
            None => {}
        }
        let remap = params.mode.is_remap();
        let keyframe = self.pending_keyframe || self.keyed_remap != Some(remap);
        let step = !keyframe && self.pending_step;
        self.pending_keyframe = false;
        self.pending_step = false;
        if keyframe {
            if remap {
                ops.push(Op::KeyCopy);
            }
            ops.push(Op::Step {
                keyframe: true,
                residual: 0.0,
            });
            self.keyed_remap = Some(remap);
        } else if step {
            let residual = if fresh_pair && !remap {
                params.residual
            } else {
                0.0
            };
            ops.push(Op::Step {
                keyframe: false,
                residual,
            });
        }
        ops.push(Op::Output);
        ops
    }

    /// Open stage `index` of a `total`-stage chain rendering into `target`
    /// at `w` x `h`, and return the full-target rect to draw into. Each
    /// stage is parented to the next (a child pass runs before its parent),
    /// and the last hangs off the pass being drawn.
    fn begin_stage(
        &mut self,
        cx: &mut Cx2d,
        index: usize,
        total: usize,
        target: &Texture,
        w: usize,
        h: usize,
    ) -> Rect {
        let size = dvec2(w as f64, h as f64);
        let parent = (index + 1 < total).then(|| self.stages[index + 1].pass.draw_pass_id());
        let st = &mut self.stages[index];
        st.pass.set_size(cx, size);
        st.pass.clear_color_textures(cx.cx);
        st.pass.set_color_texture(
            cx,
            target,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)),
        );
        match parent {
            // No attaching draw list: a link inside the chain lives as long
            // as the chain's last stage does.
            Some(parent_id) => cx
                .cx
                .attach_child_pass(st.pass.draw_pass_id(), parent_id, None),
            None => cx.make_child_pass(&st.pass),
        }
        cx.begin_pass(&st.pass, Some(1.0));
        st.pass.set_size(cx, size);
        st.pass.set_dpi_factor(cx, 1.0);
        st.draw_list.begin_always(cx);
        // A pass-local turtle: inside the host's turtle the quad would
        // inherit its on-screen clip and lose rows.
        let pass_size = cx.current_pass_size();
        cx.begin_root_turtle(pass_size, Layout::flow_overlay());
        Rect {
            pos: dvec2(0.0, 0.0),
            size,
        }
    }

    fn end_stage(&mut self, cx: &mut Cx2d, index: usize) {
        cx.end_pass_sized_turtle();
        let st = &mut self.stages[index];
        st.draw_list.end(cx);
        cx.end_pass(&st.pass);
    }

    /// Run everything queued since the last render, then the output pass.
    /// Call once per display frame from the host's `draw_walk`.
    pub fn render(&mut self, cx: &mut Cx2d) {
        let (w, h) = self.frame_size;
        if w == 0 || h == 0 {
            self.upstream.clear();
            return;
        }
        self.ensure_targets(cx.cx);
        let params = self.effective_params();
        let ops = self.plan(&params);
        // After the plan: a keyframe in it rolls a new Random drift.
        let drift_code = self.drift_pick(params.drift_mode).code();
        while self.stages.len() < ops.len() {
            self.stages.push(Stage {
                pass: DrawPass::new(cx.cx),
                draw_list: DrawList::new(cx.cx),
            });
        }
        let Some(targets) = self.targets.take() else {
            return;
        };
        let latest = self.latest_motion;
        let picture = self.picture.clone().unwrap_or_else(|| {
            if self.motion_frames > 0 {
                targets.motion[latest].clone()
            } else {
                targets.blank.clone()
            }
        });
        let (gw, gh) = self.grid();
        let total = ops.len();
        for (index, op) in ops.into_iter().enumerate() {
            match op {
                Op::Ingest { src, slot } => {
                    let r = self.begin_stage(cx, index, total, &targets.motion[slot], w, h);
                    self.draw_ingest.draw_vars.set_texture(0, &src);
                    self.draw_ingest.draw_abs(cx, r);
                }
                Op::Luma { cur } => {
                    let r = self.begin_stage(cx, index, total, &targets.luma[0], gw, gh);
                    self.draw_luma.inv_src = vec2(1.0 / w as f32, 1.0 / h as f32);
                    self.draw_luma
                        .draw_vars
                        .set_texture(0, &targets.motion[cur]);
                    self.draw_luma
                        .draw_vars
                        .set_texture(1, &targets.motion[cur ^ 1]);
                    self.draw_luma.draw_abs(cx, r);
                }
                Op::Halve { level } => {
                    let (lw, lh) = self.level_dims(level);
                    let r = self.begin_stage(cx, index, total, &targets.luma[level], lw, lh);
                    self.draw_halve
                        .draw_vars
                        .set_texture(0, &targets.luma[level - 1]);
                    self.draw_halve.draw_abs(cx, r);
                }
                Op::Search => {
                    let (tw, th) = self.level_dims(LEVELS - 1);
                    let r = self.begin_stage(cx, index, total, &targets.scratch[2], tw, th);
                    self.draw_search.inv_size = vec2(1.0 / tw as f32, 1.0 / th as f32);
                    self.draw_search.radius = SEARCH_RADIUS;
                    self.draw_search
                        .draw_vars
                        .set_texture(0, &targets.luma[LEVELS - 1]);
                    self.draw_search.draw_abs(cx, r);
                }
                Op::Refine { level, sweep } => {
                    let (lw, lh) = self.level_dims(level);
                    // The first sweep of a level starts from the coarser
                    // level's median (or the top search); later sweeps
                    // ping-pong.
                    let (prev, prev_scale) = if sweep > 0 {
                        (&targets.scratch[(sweep - 1) & 1], vec2(1.0, 1.0))
                    } else if level + 1 < LEVELS {
                        let (cw, ch) = self.level_dims(level + 1);
                        (
                            &targets.scratch[2],
                            vec2(lw as f32 / cw as f32, lh as f32 / ch as f32),
                        )
                    } else {
                        (&targets.scratch[2], vec2(1.0, 1.0))
                    };
                    let r = self.begin_stage(cx, index, total, &targets.scratch[sweep & 1], lw, lh);
                    self.draw_refine.inv_size = vec2(1.0 / lw as f32, 1.0 / lh as f32);
                    self.draw_refine.prev_scale = prev_scale;
                    self.draw_refine
                        .draw_vars
                        .set_texture(0, &targets.luma[level]);
                    self.draw_refine.draw_vars.set_texture(1, prev);
                    self.draw_refine.draw_abs(cx, r);
                }
                Op::Median { level } => {
                    let (lw, lh) = self.level_dims(level);
                    let r = self.begin_stage(cx, index, total, &targets.scratch[2], lw, lh);
                    self.draw_median.inv_size = vec2(1.0 / lw as f32, 1.0 / lh as f32);
                    self.draw_median
                        .draw_vars
                        .set_texture(0, &targets.scratch[(SWEEPS - 1) & 1]);
                    self.draw_median.draw_abs(cx, r);
                }
                Op::Subpel => {
                    let r = self.begin_stage(cx, index, total, &targets.field_est, gw, gh);
                    self.draw_subpel.inv_size = vec2(1.0 / gw as f32, 1.0 / gh as f32);
                    self.draw_subpel.draw_vars.set_texture(0, &targets.luma[0]);
                    self.draw_subpel
                        .draw_vars
                        .set_texture(1, &targets.scratch[2]);
                    self.draw_subpel.draw_abs(cx, r);
                }
                Op::Vectors { src, format } => {
                    let r = self.begin_stage(cx, index, total, &targets.field_sup, w, h);
                    let d = &mut self.draw_vectors;
                    d.vec_scale = vec2(format.scale[0], format.scale[1]);
                    d.vec_sign = if format.kind == VectorKind::Backward {
                        -1.0
                    } else {
                        1.0
                    };
                    d.absolute = if format.kind == VectorKind::PreviousPosition {
                        1.0
                    } else {
                        0.0
                    };
                    d.flip_y = if format.y_up { 1.0 } else { 0.0 };
                    d.draw_vars.set_texture(0, &src);
                    d.draw_abs(cx, r);
                }
                Op::KeyCopy => {
                    let r = self.begin_stage(cx, index, total, &targets.key, w, h);
                    self.draw_ingest.draw_vars.set_texture(0, &picture);
                    self.draw_ingest.draw_abs(cx, r);
                }
                Op::Step { keyframe, residual } => {
                    let back = self.front ^ 1;
                    let r = self.begin_stage(cx, index, total, &targets.reference[back], w, h);
                    self.steps = self.steps.wrapping_add(1);
                    let field = match self.field {
                        FieldSource::None => &targets.blank,
                        FieldSource::Estimated => &targets.field_est,
                        FieldSource::Supplied => &targets.field_sup,
                    };
                    let d = &mut self.draw_step;
                    d.frame_size = vec2(w as f32, h as f32);
                    d.inv_frame = vec2(1.0 / w as f32, 1.0 / h as f32);
                    d.block = params.block_size.max(1.0);
                    d.gain = params.gain;
                    d.motion_mat = vec4(
                        params.matrix[0],
                        params.matrix[1],
                        params.matrix[2],
                        params.matrix[3],
                    );
                    d.drift = params.drift;
                    d.drift_mode = drift_code;
                    d.diffusion = params.diffusion.max(0.0);
                    d.pel = params.pel.max(0.0);
                    d.refresh = params.refresh.clamp(0.0, 1.0);
                    d.heal = params.heal.clamp(0.0, 1.0);
                    d.residual = residual;
                    d.entropy = params.entropy.max(0.0);
                    // Small and cycling: the hash wants modest inputs.
                    d.seed = (self.steps % 1024) as f32;
                    d.remap = if params.mode.is_remap() { 1.0 } else { 0.0 };
                    d.keyframe = if keyframe { 1.0 } else { 0.0 };
                    d.field_on = if self.field == FieldSource::None {
                        0.0
                    } else {
                        1.0
                    };
                    d.draw_vars.set_texture(0, &targets.reference[self.front]);
                    d.draw_vars.set_texture(1, field);
                    d.draw_vars.set_texture(2, &picture);
                    d.draw_vars.set_texture(3, &targets.motion[latest]);
                    d.draw_vars.set_texture(4, &targets.motion[latest ^ 1]);
                    d.draw_abs(cx, r);
                    self.front = back;
                }
                Op::Output => {
                    let r = self.begin_stage(cx, index, total, &targets.output, w, h);
                    let field = match self.field {
                        FieldSource::None => &targets.blank,
                        FieldSource::Estimated => &targets.field_est,
                        FieldSource::Supplied => &targets.field_sup,
                    };
                    let d = &mut self.draw_output;
                    d.frame_size = vec2(w as f32, h as f32);
                    d.remap = if self.keyed_remap == Some(true) {
                        1.0
                    } else {
                        0.0
                    };
                    d.live_on = if params.mode == MoshMode::RemapLive {
                        1.0
                    } else {
                        0.0
                    };
                    d.wet = params.wet.clamp(0.0, 1.0);
                    d.inv_frame = vec2(1.0 / w as f32, 1.0 / h as f32);
                    d.block = params.block_size.max(1.0);
                    d.gain = params.gain;
                    d.motion_mat = vec4(
                        params.matrix[0],
                        params.matrix[1],
                        params.matrix[2],
                        params.matrix[3],
                    );
                    d.drift = params.drift;
                    d.drift_mode = drift_code;
                    d.field_on = if self.field == FieldSource::None { 0.0 } else { 1.0 };
                    d.blur_motion = params.blur_motion.max(0.0);
                    d.blur_drift = params.blur_drift.max(0.0);
                    d.view_mode = match params.view {
                        MoshView::Output => 0.0,
                        MoshView::Vectors => 1.0,
                        MoshView::Damage => 2.0,
                    };
                    d.draw_vars.set_texture(0, &targets.reference[self.front]);
                    d.draw_vars.set_texture(1, &picture);
                    d.draw_vars.set_texture(2, &targets.key);
                    d.draw_vars.set_texture(3, field);
                    d.draw_abs(cx, r);
                }
            }
            self.end_stage(cx, index);
        }
        self.targets = Some(targets);
        let head = self.stages[0].pass.draw_pass_id();
        for pass_id in self.upstream.drain(..) {
            cx.cx.attach_child_pass(pass_id, head, None);
        }
        self.rendered = true;
    }
}

/// [`Datamosh`] as a widget: drop a `DatamoshView{}` into a Splash tree,
/// drive it through [`DatamoshViewRef`] (it derefs to the engine) and show
/// [`Datamosh::output_texture`] wherever the host likes. It draws nothing
/// on screen itself.
#[derive(Script, ScriptHook, WidgetRef, WidgetRegister)]
pub struct DatamoshView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[deref]
    mosh: Datamosh,
    #[rust]
    area: Area,
}

impl WidgetNode for DatamoshView {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }
    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }
    fn area(&self) -> Area {
        self.area
    }
    fn redraw(&mut self, cx: &mut Cx) {
        self.area.redraw(cx);
    }
}

impl Widget for DatamoshView {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.walk_turtle_with_area(&mut self.area, walk);
        self.mosh.render(cx);
        DrawStep::done()
    }
}
