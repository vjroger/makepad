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
//! "Edge cut" drops points whose depth neighbourhood spans more than that
//! fraction of their own depth: the smeared "flying pixels" a depth model
//! puts between a foreground edge and the background.

use crate::depth::DepthMap;
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
        // Normalized disparity: 1 = near plane, 0 = far plane, < 0 = none.
        tex_depth: texture_2d(float)

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

        v_color: varying(vec3f)

        cull: fn() {
            self.v_color = vec3(0.0, 0.0, 0.0)
            self.vertex_pos = vec4(2.0, 2.0, 2.0, 1.0)
        }

        depth_at: fn(n: float) -> float {
            return 1.0 / (self.inv_range.y + (self.inv_range.x - self.inv_range.y) * n)
        }

        depth_tap: fn(uv: vec2) -> float {
            return self.tex_depth.sample_nearest(uv, 0.0).x
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
            let n00 = self.depth_tap(t00)
            let n10 = self.depth_tap(t00 + tx)
            let n01 = self.depth_tap(t00 + ty)
            let n11 = self.depth_tap(t00 + tx + ty)
            let n_lo = min(min(n00, n10), min(n01, n11))
            if n_lo < 0.0 {
                self.cull()
                return
            }
            let n = mix(mix(n00, n10, fx), mix(n01, n11, fx), fy)
            let z = self.depth_at(n)

            if self.edge_cut > 0.0 {
                let rx = vec2(self.edge_radius * self.depth_texel.z, 0.0)
                let ry = vec2(0.0, self.edge_radius * self.depth_texel.w)
                let na = self.depth_tap(cell + rx)
                let nb = self.depth_tap(cell - rx)
                let nc = self.depth_tap(cell + ry)
                let nd = self.depth_tap(cell - ry)
                let lo = min(n_lo, min(min(na, nb), min(nc, nd)))
                let hi = max(max(max(n00, n10), max(n01, n11)), max(max(na, nb), max(nc, nd)))
                let z_near = self.depth_at(hi)
                if lo < 0.0 || self.depth_at(lo) - z_near > self.edge_cut * z_near {
                    self.cull()
                    return
                }
            }

            // Unproject through the origin pinhole (camera looks down -z, +y up).
            let ndc = vec2(cell.x * 2.0 - 1.0, 1.0 - cell.y * 2.0)
            let world = vec4(ndc.x * self.tan_half.x * z, ndc.y * self.tan_half.y * z, -z, 1.0)
            let view = self.draw_pass.camera_view * world
            // Camera-facing billboard covering exactly one cell at depth z.
            let half_x = self.tan_half.x * self.grid.z * z * self.point_size
            let half_y = self.tan_half.y * self.grid.w * z * self.point_size
            let corner = vec4(view.x + quad.x * half_x, view.y + quad.y * half_y, view.z, view.w)
            self.vertex_pos = self.draw_pass.camera_projection * corner

            // NV12, BT.709 limited range.
            let cuv = vec2(
                self.picture_rect.x + cell.x * self.picture_rect.z,
                self.picture_rect.y + cell.y * self.picture_rect.w
            )
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
}

impl DepthCloud {
    /// Distance from the capture camera to the orbit pivot (the near/far
    /// midpoint in disparity), for the camera framing.
    pub fn pivot_distance(depth_amount: f32) -> f32 {
        let far = depth_amount.max(1.0);
        2.0 * far / (1.0 + far)
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
        self.draw_cloud.redraw(cx);
    }

    fn set_draw_uniforms(&mut self, cx: &mut CxDraw, scene: &SceneState3D, cols: usize, rows: usize) {
        let Some(textures) = self.textures.as_ref() else {
            return;
        };
        let (fw, fh) = textures.frame_size;
        let (dw, dh) = textures.depth_size;
        let rect = self.picture_rect;
        let aspect = (fw as f32 * rect[2]) / (fh as f32 * rect[3]).max(1.0);
        // Same vertical FOV as the scene camera: the front view is the video.
        let tan_y = 1.0 / scene.projection.v[5].abs().max(0.00001);
        let far = self.depth_amount.max(1.0);
        let dv = &mut self.draw_cloud.draw_vars;
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
        dv.set_texture(0, &textures.y);
        dv.set_texture(1, &textures.uv);
        dv.set_texture(2, &textures.depth);
    }
}

impl Widget for DepthCloud {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_3d(&mut self, cx: &mut Cx3d, _scope: &mut Scope) -> DrawStep {
        let Some(scene) = cx.scene_state_3d() else {
            return DrawStep::done();
        };
        let Some(textures) = self.textures.as_ref() else {
            return DrawStep::done();
        };
        if !self.has_depth {
            return DrawStep::done();
        }
        let (fw, fh) = textures.frame_size;
        let aspect = (fw as f32 * self.picture_rect[2]) / (fh as f32 * self.picture_rect[3]).max(1.0);
        let cols = (self.points_per_row.round() as usize).clamp(8, 2048);
        let rows = ((cols as f32 / aspect.max(0.01)).round() as usize).clamp(1, 2048);
        let count = cols * rows;
        if self.instance_ids.len() != count {
            self.instance_ids = (0..count).map(|i| i as f32).collect();
        }
        self.set_draw_uniforms(cx, &scene, cols, rows);
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
