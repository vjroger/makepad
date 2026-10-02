//! The point cloud: one instanced billboard per grid cell, placed by depth.
//!
//! Nothing per point lives on the CPU: the instance stream is just the cell
//! index, and the vertex shader reads the depth map (manual bilinear, so the
//! edge test sees the same texels) and the NV12 colour planes, then
//! unprojects the cell through a pinhole camera at the world origin looking
//! down -z. The pinhole uses the SCENE camera's vertical FOV, so with the
//! orbit camera at its front pose every billboard lands exactly on its own
//! cell of the video and the cloud reads as the flat video; billboards are
//! sized to one cell at their depth, so the front view has no gaps.
//!
//! Depth arrives in one of three encodings (`depth_mode`): the pipeline's
//! normalized disparity (models, packed RGBD, depth-pass videos), or, for
//! content a GPU renders live, that renderer's own targets with no copy and
//! no model: a linear view-depth target ([`RenderedDepth`]). Colour is
//! NV12 planes or the renderer's colour target.
//!
//! The widget plays its own source: [`DepthCloud::open`] starts a
//! [`Pipeline`] (decode, depth, stabilize on a worker) and the widget pumps
//! its frames, reporting [`DepthCloudAction`]s. Drop a `DepthCloud{}` into
//! any `XrSceneView` and call `open`, or hand it live GPU targets with
//! [`DepthCloud::set_rendered_source`].
//!
//! Shaping: a depth crop band (vertex shader), and mouse effects
//! ([`CloudEffect`]: attract, repel, swirl, ripple) that push the points
//! around the cursor; each point has momentum and springs back home with
//! damping (`crate::sim`, offsets sampled by the vertex shader).
//!
//! "Edge cut" drops points whose depth neighbourhood spans more than that
//! fraction of their own depth: the smeared "flying pixels" a depth model
//! puts between a foreground edge and the background.

use crate::depth::DepthMap;
use crate::pipeline::{FrameStats, Pipeline, PipelineSettings, SourceSpec};
use crate::sim::{PointSim, RestDepth, SimParams};
use makepad_widgets::{makepad_derive_widget::*, makepad_draw::*, widget::*};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.math.*
    use mod.shader.*
    use mod.draw
    use mod.geom

    mod.draw.DrawDepthCloud = set_type_default() do #(DrawDepthCloud::script_shader(vm)){
        alpha_blend: false
        depth_write: true
        backface_culling: false
        vertex_pos: vertex_position(vec4f)
        fb0: fragment_output(0, vec4f)
        draw_call: uniform_buffer(draw.DrawCallUniforms)
        draw_pass: uniform_buffer(draw.DrawPassUniforms)
        draw_list: uniform_buffer(draw.DrawListUniforms)
        geom: vertex_buffer(geom.QuadVertex, geom.QuadGeom)
        // NV12 colour planes (R8 luma, RG8 interleaved chroma at half size).
        tex_y: texture_2d(float)
        tex_uv: texture_2d(float)
        // depth_mode 0: normalized disparity (1 = near, 0 = far, < 0 = none);
        // 1: linear view depth of a rendered source (<= 0 = none).
        tex_depth: texture_2d(float)
        // A renderer's colour target (color_mode 1).
        tex_color: texture_2d(float)
        // Per-point physics offsets (one RGBA f32 texel per grid cell).
        tex_offset: texture_2d(float)

        // x,y: points per row / rows; z,w: their reciprocals.
        grid: uniform(vec4(256.0, 144.0, 0.00390625, 0.0069444))
        // Picture region of the colour planes: u0, v0, du, dv.
        picture_rect: uniform(vec4(0.0, 0.0, 1.0, 1.0))
        // x,y: depth map size; z,w: texel size.
        depth_texel: uniform(vec4(2.0, 2.0, 0.5, 0.5))
        // tan(fov/2) horizontally and vertically for the picture.
        tan_half: uniform(vec2(0.7, 0.4))
        // 1/near, 1/far.
        inv_range: uniform(vec2(1.0, 0.25))
        point_size: uniform(1.0)
        edge_cut: uniform(0.08)
        edge_radius: uniform(1.5)
        depth_mode: uniform(0.0)
        // Rendered sources: x,y = the scene's near/far (scene units, for the
        // crop), z = scene depth units -> cloud units.
        depth_params: uniform(vec4(0.1, 100.0, 1.0, 0.0))
        color_mode: uniform(0.0)
        // Keep points inside this band of the SOURCE's depth range
        // (0 = nearest, 1 = farthest), independent of the depth amount.
        crop: uniform(vec2(-1.0, 2.0))
        // 1 = add tex_offset (points pushed by a mouse effect).
        use_offset: uniform(0.0)

        v_color: varying(vec3f)

        cull: fn() {
            self.v_color = vec3(0.0, 0.0, 0.0)
            self.vertex_pos = vec4(2.0, 2.0, 2.0, 1.0)
        }

        depth_at: fn(n: float) -> float {
            return 1.0 / (self.inv_range.y + (self.inv_range.x - self.inv_range.y) * n)
        }

        // View depth at `uv` in cloud units; negative = no point there.
        depth_tap: fn(uv: vec2) -> float {
            let d = self.tex_depth.sample_nearest(uv, 0.0).x
            if self.depth_mode < 0.5 {
                if d < 0.0 {
                    return -1.0
                }
                return self.depth_at(d)
            }
            if d <= 0.0 {
                return -1.0
            }
            return d * self.depth_params.z
        }

        vertex: fn() {
            let quad = self.geom.pos * 2.0 - vec2(1.0, 1.0)
            let id = self.point_id
            let row = floor((id + 0.5) * self.grid.z)
            let col = id - row * self.grid.x
            let cell = vec2((col + 0.5) * self.grid.z, (row + 0.5) * self.grid.w)

            // Manual bilinear over the 2x2 texels around the cell centre.
            let dp = vec2(cell.x * self.depth_texel.x - 0.5, cell.y * self.depth_texel.y - 0.5)
            let base = vec2(
                clamp(floor(dp.x), 0.0, max(self.depth_texel.x - 2.0, 0.0)),
                clamp(floor(dp.y), 0.0, max(self.depth_texel.y - 2.0, 0.0))
            )
            let fx = clamp(dp.x - base.x, 0.0, 1.0)
            let fy = clamp(dp.y - base.y, 0.0, 1.0)
            let t00 = vec2((base.x + 0.5) * self.depth_texel.z, (base.y + 0.5) * self.depth_texel.w)
            let tx = vec2(self.depth_texel.z, 0.0)
            let ty = vec2(0.0, self.depth_texel.w)
            let z00 = self.depth_tap(t00)
            let z10 = self.depth_tap(t00 + tx)
            let z01 = self.depth_tap(t00 + ty)
            let z11 = self.depth_tap(t00 + tx + ty)
            let z_lo = min(min(z00, z10), min(z01, z11))
            if z_lo < 0.0 {
                self.cull()
                return
            }
            let z = mix(mix(z00, z10, fx), mix(z01, z11, fx), fy)
            // Where the point sits in the source's own range, 0 = nearest:
            // the stabilized disparity for frame sources, the scene's
            // near/far (in disparity) for rendered ones.
            let raw = self.tex_depth.sample_nearest(cell, 0.0).x
            var far_frac = 1.0 - raw
            if self.depth_mode > 0.5 {
                let inv_near = 1.0 / max(self.depth_params.x, 0.00001)
                let inv_far = 1.0 / max(self.depth_params.y, 0.00001)
                far_frac = (inv_near - 1.0 / max(raw, 0.00001)) / max(inv_near - inv_far, 0.00001)
            }
            if far_frac < self.crop.x || far_frac > self.crop.y {
                self.cull()
                return
            }

            if self.edge_cut > 0.0 {
                let rx = vec2(self.edge_radius * self.depth_texel.z, 0.0)
                let ry = vec2(0.0, self.edge_radius * self.depth_texel.w)
                let za = self.depth_tap(cell + rx)
                let zb = self.depth_tap(cell - rx)
                let zc = self.depth_tap(cell + ry)
                let zd = self.depth_tap(cell - ry)
                let lo = min(z_lo, min(min(za, zb), min(zc, zd)))
                let hi = max(max(max(z00, z10), max(z01, z11)), max(max(za, zb), max(zc, zd)))
                if lo < 0.0 || hi - lo > self.edge_cut * lo {
                    self.cull()
                    return
                }
            }

            // Unproject through the origin pinhole (camera looks down -z, +y up).
            let ndc = vec2(cell.x * 2.0 - 1.0, 1.0 - cell.y * 2.0)
            var wp = vec3(ndc.x * self.tan_half.x * z, ndc.y * self.tan_half.y * z, -z)
            if self.use_offset > 0.5 {
                let offset = self.tex_offset.sample_nearest(cell, 0.0)
                wp = wp + offset.xyz
            }
            let view = self.draw_pass.camera_view * vec4(wp.x, wp.y, wp.z, 1.0)
            // Camera-facing billboard covering exactly one cell at depth z.
            let half_x = self.tan_half.x * self.grid.z * z * self.point_size
            let half_y = self.tan_half.y * self.grid.w * z * self.point_size
            let corner = vec4(view.x + quad.x * half_x, view.y + quad.y * half_y, view.z, view.w)
            self.vertex_pos = self.draw_pass.camera_projection * corner

            let cuv = vec2(
                self.picture_rect.x + cell.x * self.picture_rect.z,
                self.picture_rect.y + cell.y * self.picture_rect.w
            )
            if self.color_mode > 0.5 {
                let c = self.tex_color.sample_lod(cuv, 0.0)
                self.v_color = vec3(c.x, c.y, c.z)
                return
            }
            // NV12, BT.709 limited range.
            let yv = self.tex_y.sample_lod(cuv, 0.0).x
            let chroma = self.tex_uv.sample_lod(cuv, 0.0)
            let y = (yv * 255.0 - 16.0) / 219.0
            let u = (chroma.x * 255.0 - 128.0) / 224.0
            let v = (chroma.y * 255.0 - 128.0) / 224.0
            self.v_color = vec3(
                clamp(y + 1.5748 * v, 0.0, 1.0),
                clamp(y - 0.1873 * u - 0.4681 * v, 0.0, 1.0),
                clamp(y + 1.8556 * u, 0.0, 1.0)
            )
        }

        pixel: fn() {
            return vec4(self.v_color.x, self.v_color.y, self.v_color.z, 1.0)
        }

        fragment: fn() {
            self.fb0 = self.pixel()
        }
    }

    mod.widgets.DepthCloudBase = #(DepthCloud::register_widget(vm))
    mod.widgets.DepthCloud = set_type_default() do mod.widgets.DepthCloudBase{
        points_per_row: 384.0
        point_size: 1.15
        edge_cut: 0.08
        edge_radius: 1.5
        depth_amount: 4.0
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawDepthCloud {
    #[deref]
    pub draw_vars: DrawVars,
    /// Instance stream: grid cell index (exact integer in f32).
    #[live(0.0)]
    pub point_id: f32,
}

/// What the mouse does to the points around the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloudEffect {
    #[default]
    Off,
    Attract,
    Repel,
    Swirl,
    Ripple,
}

impl CloudEffect {
    pub const ALL: [CloudEffect; 5] = [Self::Off, Self::Attract, Self::Repel, Self::Swirl, Self::Ripple];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Attract => "Attract",
            Self::Repel => "Repel",
            Self::Swirl => "Swirl",
            Self::Ripple => "Ripple",
        }
    }
}

/// What the widget reports to its host.
#[derive(Clone, Debug, Default)]
pub enum DepthCloudAction {
    #[default]
    None,
    /// A human-readable status line (source opened, model loaded, errors).
    Status(String),
    /// Playback statistics, a few times a second.
    Stats(FrameStats),
}

/// How a renderer's depth target encodes depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderedDepth {
    /// View-space distance along the camera axis, in scene units (an R32F
    /// target written by the scene's own shaders or a depth pass; 0 = empty).
    /// Portable to every backend.
    Linear,
}

/// Live GPU targets of something already rendered: used as-is, no copy.
struct RenderedSource {
    color: Texture,
    depth: Texture,
    size: (usize, usize),
    encoding: RenderedDepth,
    scale: f32,
    /// The scene's near/far in its own units: what the crop band spans.
    depth_range: (f32, f32),
}

struct CloudTextures {
    y: Texture,
    uv: Texture,
    depth: Texture,
    frame_size: (usize, usize),
    depth_size: (usize, usize),
}

#[derive(Script, ScriptHook, Widget)]
pub struct DepthCloud {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,

    #[redraw]
    #[live]
    draw_cloud: DrawDepthCloud,

    /// Points across the picture; rows follow from its aspect.
    #[live(384.0)]
    pub points_per_row: f32,
    /// Billboard size in cells (1 = exactly gap-free from the front).
    #[live(1.15)]
    pub point_size: f32,
    /// Relative depth jump that culls a point as a flying pixel (0 = off).
    #[live(0.08)]
    pub edge_cut: f32,
    /// Edge-test reach in depth texels.
    #[live(1.5)]
    pub edge_radius: f32,
    /// Far plane over near plane: how deep the scene is (1 = flat).
    #[live(4.0)]
    pub depth_amount: f32,

    #[rust]
    textures: Option<CloudTextures>,
    #[rust]
    has_depth: bool,
    #[rust([0.0, 0.0, 1.0, 1.0])]
    picture_rect: [f32; 4],
    #[rust]
    instance_ids: Vec<f32>,
    #[rust]
    rendered: Option<RenderedSource>,
    /// Bound to texture slots the current source does not use.
    #[rust]
    dummy: Option<Texture>,

    /// Keep only points inside this band of the source's own depth range:
    /// 0 = nearest, 1 = farthest. Unaffected by `depth_amount`.
    #[rust((0.0, 1.0))]
    pub crop: (f32, f32),
    #[rust]
    pub effect: CloudEffect,
    #[rust(0.6)]
    pub effect_strength: f32,
    /// How hard points are pulled home (1/s^2) and how fast their motion
    /// dies out (1/s): low damping wobbles, high damping creeps back.
    #[rust(40.0)]
    pub spring: f32,
    /// On: points have momentum (spring + damping). Off: the effect bends
    /// them directly and they snap home when the mouse leaves.
    #[rust(true)]
    pub momentum: bool,
    #[rust(5.0)]
    pub damping: f32,
    #[rust]
    sim: PointSim,
    /// CPU copy of the current depth map, for the physics' rest positions.
    #[rust]
    depth_cpu: Option<(usize, usize, Vec<f32>)>,
    /// Effector reach in cloud units.
    #[rust(0.35)]
    pub effect_radius: f32,
    /// Rendered sources only (their depth lives on the GPU): the depth the
    /// physics assumes the points rest at; `None` = the scene's middle.
    #[rust]
    pub effect_depth: Option<f32>,
    #[rust]
    mouse: Option<DVec2>,

    #[rust]
    pipeline: Option<Pipeline>,
    #[rust]
    settings: PipelineSettings,
    #[rust]
    pump: NextFrame,
    #[rust]
    stats_at: f64,
}

impl DepthCloud {
    /// Distance from the capture camera to the orbit pivot (the near/far
    /// midpoint in disparity), for the camera framing.
    pub fn pivot_distance(depth_amount: f32) -> f32 {
        let far = depth_amount.max(1.0);
        2.0 * far / (1.0 + far)
    }

    /// Play `spec` (a video, a depth model, ...): decoding and depth run on
    /// a worker; progress arrives as [`DepthCloudAction`]s.
    pub fn open(&mut self, cx: &mut Cx, spec: SourceSpec) -> Result<(), String> {
        self.close();
        self.rendered = None;
        self.has_depth = false;
        self.picture_rect = spec.layout.picture_rect();
        let pipeline = Pipeline::start(cx.task_pool(), spec, self.settings)?;
        self.pipeline = Some(pipeline);
        self.pump = cx.new_next_frame();
        // The cloud draws inside its XrSceneView's own pass: redraw the whole
        // tree so that pass re-renders too.
        cx.redraw_all();
        Ok(())
    }

    /// Stop playback (the worker exits on its own).
    pub fn close(&mut self) {
        self.pipeline = None;
    }

    pub fn settings(&self) -> PipelineSettings {
        self.settings
    }

    /// Depth speed / accuracy and stabilization knobs, applied live.
    pub fn set_settings(&mut self, settings: PipelineSettings) {
        self.settings = settings;
        if let Some(pipeline) = self.pipeline.as_mut() {
            pipeline.set_settings(settings);
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        if let Some(pipeline) = &self.pipeline {
            pipeline.set_paused(paused);
        }
    }

    pub fn is_playing(&self) -> bool {
        self.pipeline.as_ref().is_some_and(|p| !p.failed())
    }

    /// Show something already rendered on the GPU (a 3D scene's colour
    /// target and its depth): the cheap path, no model and no readback.
    /// `size` is the targets' pixel size; `scale` maps scene depth units to
    /// cloud units (about 1 = the near end of the interesting range);
    /// `depth_range` is the scene's near/far in its own units (the crop band).
    /// Replaces the frame source.
    pub fn set_rendered_source(
        &mut self,
        cx: &mut Cx,
        color: &Texture,
        depth: &Texture,
        size: (usize, usize),
        encoding: RenderedDepth,
        scale: f32,
        depth_range: (f32, f32),
    ) {
        self.close();
        self.rendered = Some(RenderedSource {
            color: color.clone(),
            depth: depth.clone(),
            size: (size.0.max(2), size.1.max(2)),
            encoding,
            scale,
            depth_range,
        });
        // The cloud draws inside its XrSceneView's own pass: redraw the whole
        // tree so that pass re-renders too.
        cx.redraw_all();
    }

    /// Back to the frame source (models, packed RGBD, depth passes).
    pub fn clear_rendered_source(&mut self, cx: &mut Cx) {
        self.rendered = None;
        // The cloud draws inside its XrSceneView's own pass: redraw the whole
        // tree so that pass re-renders too.
        cx.redraw_all();
    }

    /// Picture pixel size of the active source, `None` when nothing to draw.
    fn picture_size(&self) -> Option<(f32, f32)> {
        if let Some(rendered) = &self.rendered {
            return Some((rendered.size.0 as f32, rendered.size.1 as f32));
        }
        let textures = self.textures.as_ref().filter(|_| self.has_depth)?;
        let (fw, fh) = textures.frame_size;
        Some((fw as f32 * self.picture_rect[2], fh as f32 * self.picture_rect[3]))
    }

    pub fn set_picture_rect(&mut self, rect: [f32; 4]) {
        self.picture_rect = rect;
    }

    /// Upload one frame's colour planes and, when present, its depth map.
    pub fn push_frame(
        &mut self,
        cx: &mut Cx,
        width: usize,
        height: usize,
        nv12: &[u8],
        depth: Option<DepthMap>,
    ) {
        let y_len = width * height;
        let uv_len = (width / 2) * (height / 2) * 2;
        if width < 2 || height < 2 || nv12.len() < y_len + uv_len {
            return;
        }
        let textures = self.textures.get_or_insert_with(|| CloudTextures {
            y: Texture::new(cx),
            uv: Texture::new(cx),
            depth: Texture::new(cx),
            frame_size: (0, 0),
            depth_size: (0, 0),
        });
        if textures.frame_size != (width, height) {
            textures.y = Texture::new_with_format(
                cx,
                TextureFormat::VecRu8 {
                    width,
                    height,
                    data: Some(vec![0; y_len]),
                    unpack_row_length: None,
                    updated: TextureUpdated::Full,
                },
            );
            textures.uv = Texture::new_with_format(
                cx,
                TextureFormat::VecRGu8 {
                    width: width / 2,
                    height: height / 2,
                    data: Some(vec![128; uv_len]),
                    unpack_row_length: None,
                    updated: TextureUpdated::Full,
                },
            );
            textures.frame_size = (width, height);
        }
        let mut buf = textures.y.take_vec_u8(cx);
        buf.clear();
        buf.extend_from_slice(&nv12[..y_len]);
        textures.y.put_back_vec_u8(cx, buf, None);
        let mut buf = textures.uv.take_vec_u8(cx);
        buf.clear();
        buf.extend_from_slice(&nv12[y_len..y_len + uv_len]);
        textures.uv.put_back_vec_u8(cx, buf, None);

        if let Some(map) = depth {
            if map.width < 2 || map.height < 2 || map.values.len() != map.width * map.height {
                return;
            }
            self.depth_cpu = Some((map.width, map.height, map.values.clone()));
            if textures.depth_size != (map.width, map.height) {
                textures.depth = Texture::new_with_format(
                    cx,
                    TextureFormat::VecRf32 {
                        width: map.width,
                        height: map.height,
                        data: Some(map.values),
                        updated: TextureUpdated::Full,
                    },
                );
                textures.depth_size = (map.width, map.height);
            } else {
                // The old buffer is dropped here, on the UI thread.
                let _ = textures.depth.take_vec_f32(cx);
                textures.depth.put_back_vec_f32(cx, map.values, None);
            }
            self.has_depth = true;
        }
        // The cloud draws inside its XrSceneView's own pass: redraw the whole
        // tree so that pass re-renders too.
        cx.redraw_all();
    }

    fn set_draw_uniforms(
        &mut self,
        cx: &mut CxDraw,
        scene: &SceneState3D,
        cols: usize,
        rows: usize,
        aspect: f32,
        offsets: Option<Texture>,
    ) {
        let dummy = self
            .dummy
            .get_or_insert_with(|| {
                Texture::new_with_format(
                    cx.cx,
                    TextureFormat::VecBGRAu8_32 {
                        width: 2,
                        height: 2,
                        data: Some(vec![0; 4]),
                        updated: TextureUpdated::Full,
                    },
                )
            })
            .clone();
        // Same vertical FOV as the scene camera: the front view is the picture.
        let tan_y = 1.0 / scene.projection.v[5].abs().max(0.00001);
        let far = self.depth_amount.max(1.0);
        let crop = self.crop;
        let dv = &mut self.draw_cloud.draw_vars;
        dv.set_texture(4, offsets.as_ref().unwrap_or(&dummy));
        dv.set_uniform(cx.cx, live_id!(use_offset), &[if offsets.is_some() { 1.0 } else { 0.0 }]);
        let ((dw, dh), rect, depth_mode, params, color_mode) = match (&self.rendered, &self.textures) {
            (Some(r), _) => {
                dv.set_texture(0, &dummy);
                dv.set_texture(1, &dummy);
                dv.set_texture(2, &r.depth);
                dv.set_texture(3, &r.color);
                let mode = match r.encoding {
                    RenderedDepth::Linear => 1.0,
                };
                let (near, far) = r.depth_range;
                (r.size, [0.0, 0.0, 1.0, 1.0], mode, [near, far, r.scale, 0.0], 1.0)
            }
            (None, Some(t)) => {
                dv.set_texture(0, &t.y);
                dv.set_texture(1, &t.uv);
                dv.set_texture(2, &t.depth);
                dv.set_texture(3, &dummy);
                (t.depth_size, self.picture_rect, 0.0, [0.0, 0.0, 1.0, 0.0], 0.0)
            }
            (None, None) => return,
        };
        dv.set_uniform(
            cx.cx,
            live_id!(grid),
            &[cols as f32, rows as f32, 1.0 / cols as f32, 1.0 / rows as f32],
        );
        dv.set_uniform(cx.cx, live_id!(picture_rect), &rect);
        dv.set_uniform(
            cx.cx,
            live_id!(depth_texel),
            &[dw as f32, dh as f32, 1.0 / dw as f32, 1.0 / dh as f32],
        );
        dv.set_uniform(cx.cx, live_id!(tan_half), &[tan_y * aspect, tan_y]);
        dv.set_uniform(cx.cx, live_id!(inv_range), &[1.0, 1.0 / far]);
        dv.set_uniform(cx.cx, live_id!(point_size), &[self.point_size]);
        dv.set_uniform(cx.cx, live_id!(edge_cut), &[self.edge_cut]);
        dv.set_uniform(cx.cx, live_id!(edge_radius), &[self.edge_radius]);
        dv.set_uniform(cx.cx, live_id!(depth_mode), &[depth_mode]);
        dv.set_uniform(cx.cx, live_id!(depth_params), &params);
        dv.set_uniform(cx.cx, live_id!(color_mode), &[color_mode]);
        // The full band never cuts, whatever the rounding at its ends.
        let lo = if crop.0 <= 0.0 { -1.0 } else { crop.0 };
        let hi = if crop.1 >= 1.0 { 2.0 } else { crop.1 };
        dv.set_uniform(cx.cx, live_id!(crop), &[lo, hi]);
    }
}

impl DepthCloud {
    /// The ray from the camera through the cursor (origin, unit direction):
    /// the effect acts on everything within `effect_radius` of it, at any
    /// depth, like a cylinder through the scene.
    fn effector(&self, scene: &SceneState3D) -> Option<(Vec3f, Vec3f)> {
        if self.effect == CloudEffect::Off {
            return None;
        }
        let abs = self.mouse?;
        let rect = scene.viewport_rect;
        if rect.size.x <= 1.0 || rect.size.y <= 1.0 || !rect.contains(abs) {
            return None;
        }
        let ndc_x = (((abs.x - rect.pos.x) / rect.size.x) * 2.0 - 1.0) as f32;
        let ndc_y = (1.0 - ((abs.y - rect.pos.y) / rect.size.y) * 2.0) as f32;
        let inv_projection = scene.projection.invert();
        let inv_view = scene.view.invert();
        let unproject = |z: f32| {
            let v = inv_projection.transform_vec4(vec4(ndc_x, ndc_y, z, 1.0));
            let v = vec4(v.x / v.w, v.y / v.w, v.z / v.w, 1.0);
            let w = inv_view.transform_vec4(v);
            vec3(w.x / w.w, w.y / w.w, w.z / w.w)
        };
        let near = unproject(-1.0);
        let far = unproject(1.0);
        let dir = far - near;
        let len = dir.length();
        (len > 1e-6).then(|| (near, dir * (1.0 / len)))
    }

    /// Camera distance to the scene's middle, where `effect_radius` applies.
    fn effect_ref_depth(&self, scene: &SceneState3D) -> f32 {
        let middle = vec3(
            0.0,
            0.0,
            -self
                .effect_depth
                .unwrap_or_else(|| Self::pivot_distance(self.depth_amount)),
        );
        let camera = scene.view.invert().transform_vec4(vec4(0.0, 0.0, 0.0, 1.0));
        let camera = vec3(camera.x, camera.y, camera.z);
        (middle - camera).length().max(0.05)
    }

    /// Advance the point physics to this frame; the offset texture while
    /// any point is away from home.
    fn step_physics(
        &mut self,
        cx: &mut Cx,
        scene: &SceneState3D,
        cols: usize,
        rows: usize,
        aspect: f32,
    ) -> Option<Texture> {
        let effector = self.effector(scene);
        if effector.is_none() && !self.sim.is_moving() {
            return None;
        }
        let tan_y = 1.0 / scene.projection.v[5].abs().max(0.00001);
        let plane = self
            .effect_depth
            .unwrap_or_else(|| Self::pivot_distance(self.depth_amount));
        let rest = match (&self.rendered, &self.depth_cpu) {
            (None, Some((width, height, values))) => RestDepth::Map {
                width: *width,
                height: *height,
                values,
                inv_far: 1.0 / self.depth_amount.max(1.0),
            },
            _ => RestDepth::Plane(plane),
        };
        let params = SimParams {
            cols,
            rows,
            rest,
            tan_half: (tan_y * aspect, tan_y),
            effector,
            effect: self.effect,
            strength: self.effect_strength,
            radius: self.effect_radius,
            spring: self.spring,
            damping: self.damping,
            momentum: self.momentum,
            ref_depth: self.effect_ref_depth(scene),
            time: scene.time,
        };
        let offsets = self.sim.step(cx, &params);
        if self.sim.is_moving() {
            self.pump = cx.new_next_frame();
        }
        offsets
    }

    fn pump_frames(&mut self, cx: &mut Cx) {
        let uid = self.uid;
        // Physics keeps frames coming while points move or the mouse pushes.
        let mut animate =
            self.sim.is_moving() || (self.effect != CloudEffect::Off && self.mouse.is_some());
        if let Some(pipeline) = self.pipeline.as_mut() {
            pipeline.flush_settings();
            if let Some(status) = pipeline.take_status() {
                cx.widget_action(uid, DepthCloudAction::Status(status));
            }
            let failed = pipeline.failed();
            if let Some(frame) = pipeline.take_frame() {
                let stats = frame.stats;
                self.push_frame(cx, frame.width, frame.height, &frame.nv12, frame.depth);
                let now = Cx::monotonic_now();
                if now - self.stats_at > 0.25 {
                    self.stats_at = now;
                    cx.widget_action(uid, DepthCloudAction::Stats(stats));
                }
            }
            animate |= !failed;
        }
        if animate {
            // The cloud draws inside its XrSceneView's own pass: redraw the whole
            // tree so that pass re-renders too.
            cx.redraw_all();
            self.pump = cx.new_next_frame();
        }
    }
}

impl Widget for DepthCloud {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        match event {
            Event::NextFrame(next) if next.set.contains(&self.pump) => self.pump_frames(cx),
            Event::MouseMove(e) => {
                self.mouse = Some(e.abs);
                if self.effect != CloudEffect::Off || self.sim.is_moving() {
                    // The cloud draws inside its XrSceneView's own pass: redraw the whole
                    // tree so that pass re-renders too.
                    cx.redraw_all();
                    self.pump = cx.new_next_frame();
                }
            }
            _ => {}
        }
    }

    fn draw_3d(&mut self, cx: &mut Cx3d, _scope: &mut Scope) -> DrawStep {
        let Some(scene) = cx.scene_state_3d() else {
            return DrawStep::done();
        };
        let Some((pw, ph)) = self.picture_size() else {
            return DrawStep::done();
        };
        let aspect = pw / ph.max(1.0);
        let cols = (self.points_per_row.round() as usize).clamp(8, 2048);
        let rows = ((cols as f32 / aspect.max(0.01)).round() as usize).clamp(1, 2048);
        let count = cols * rows;
        if self.instance_ids.len() != count {
            self.instance_ids = (0..count).map(|i| i as f32).collect();
        }
        let offsets = self.step_physics(cx.cx, &scene, cols, rows, aspect);
        self.set_draw_uniforms(cx, &scene, cols, rows, aspect, offsets);
        if let Some(mut instances) = cx.begin_many_instances(&self.draw_cloud.draw_vars) {
            instances.instances.extend_from_slice(&self.instance_ids);
            let area = cx.end_many_instances(instances);
            self.draw_cloud.draw_vars.area =
                cx.update_area_refs(self.draw_cloud.draw_vars.area, area);
        }
        DrawStep::done()
    }

    fn draw_walk(&mut self, _cx: &mut Cx2d, _scope: &mut Scope, _walk: Walk) -> DrawStep {
        DrawStep::done()
    }
}
