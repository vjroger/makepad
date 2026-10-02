//! A live 3D scene rendered on the GPU: the "already rendered" input.
//!
//! When the content is itself a render, its depth is exact and free: the
//! renderer writes it anyway. This widget ray-marches a small animated scene
//! into two offscreen targets, a colour target and an R32F target holding
//! LINEAR view depth (distance along the camera axis), and the point cloud
//! samples both directly on the GPU ([`crate::cloud::RenderedDepth::Linear`]):
//! no readback, no model, no CPU work per frame. Any renderer that can write
//! a linear-depth target (or expose a sampled depth buffer) plugs in the same
//! way through `DepthCloud::set_rendered_source`.
//!
//! The scene camera is the cloud's capture camera: at the origin, looking
//! down -z, with the same vertical FOV, so the front view matches exactly.

use makepad_widgets::{makepad_derive_widget::*, makepad_draw::*, widget::*};

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.math.*
    use mod.shader.*
    use mod.draw

    mod.draw.DrawSdfScene = set_type_default() do #(DrawSdfScene::script_shader(vm)){
        ..mod.draw.DrawQuad
        scene_time: 0.0
        output_depth: 0.0
        tan_half: vec2(0.7, 0.4)

        // Signed distance to the scene; .y = material (0 floor, 1 sphere, 2 box).
        scene: fn(p: vec3) -> vec2 {
            let t = self.scene_time
            var d = vec2(p.y + 1.0, 0.0)
            var i = 0.0
            while i < 3.0 {
                let a = t * (0.4 + 0.15 * i) + i * 2.1
                let c = vec3(sin(a) * (1.2 + 0.5 * i), -0.4 + 0.35 * abs(sin(t * 1.3 + i)), -4.5 + cos(a) * 1.4)
                let ds = length(p - c) - 0.55
                if ds < d.x {
                    d = vec2(ds, 1.0 + i * 0.1)
                }
                i = i + 1.0
            }
            let q = abs(p - vec3(0.0, -0.3, -7.0)) - vec3(1.6, 0.7, 0.4)
            let db = length(max(q, vec3(0.0, 0.0, 0.0))) + min(max(q.x, max(q.y, q.z)), 0.0)
            if db < d.x {
                d = vec2(db, 2.0)
            }
            return d
        }

        pixel: fn() {
            let ndc = vec2(self.pos.x * 2.0 - 1.0, 1.0 - self.pos.y * 2.0)
            let dir = normalize(vec3(ndc.x * self.tan_half.x, ndc.y * self.tan_half.y, -1.0))
            var t = 0.1
            var hit = vec2(-1.0, 0.0)
            var step = 0.0
            while step < 96.0 {
                let h = self.scene(dir * t)
                if h.x < 0.001 * t {
                    hit = vec2(t, h.y)
                    step = 1000.0
                } else {
                    t = t + h.x
                    if t > 30.0 {
                        step = 1000.0
                    }
                }
                step = step + 1.0
            }
            if self.output_depth > 0.5 {
                // Linear view depth (distance along -z); 0 = nothing there.
                let z = if hit.x > 0.0 { hit.x * -dir.z } else { 0.0 }
                return vec4(z, 0.0, 0.0, 1.0)
            }
            if hit.x < 0.0 {
                return vec4(0.05, 0.07, 0.1, 1.0)
            }
            let p = dir * hit.x
            let ex = vec3(0.002, 0.0, 0.0)
            let ey = vec3(0.0, 0.002, 0.0)
            let ez = vec3(0.0, 0.0, 0.002)
            let n = normalize(vec3(
                self.scene(p + ex).x - self.scene(p - ex).x,
                self.scene(p + ey).x - self.scene(p - ey).x,
                self.scene(p + ez).x - self.scene(p - ez).x
            ))
            let light = max(dot(n, normalize(vec3(0.5, 0.8, 0.4))), 0.0) * 0.8 + 0.2
            var base = vec3(0.9, 0.5, 0.2)
            if hit.y < 0.5 {
                let k = floor(p.x * 2.0) + floor(p.z * 2.0)
                let c = k - 2.0 * floor(k * 0.5)
                base = mix(vec3(0.25, 0.3, 0.35), vec3(0.75, 0.8, 0.85), c)
            } else if hit.y > 1.5 {
                base = vec3(0.3, 0.6, 0.9)
            } else {
                base = vec3(0.9, 0.45 + (hit.y - 1.0) * 2.5, 0.2)
            }
            let fog = exp(-0.03 * hit.x)
            let col = base * light * fog
            return vec4(col.x, col.y, col.z, 1.0)
        }
    }

    mod.widgets.RenderedSceneBase = #(RenderedScene::register_widget(vm))
    mod.widgets.RenderedScene = set_type_default() do mod.widgets.RenderedSceneBase{
        width: 0
        height: 0
        render_width: 960.0
        render_height: 540.0
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawSdfScene {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    scene_time: f32,
    #[live]
    output_depth: f32,
    #[live]
    tan_half: Vec2f,
}

struct Targets {
    color_pass: DrawPass,
    depth_pass: DrawPass,
    color_list: DrawList2d,
    depth_list: DrawList2d,
    color: Texture,
    depth: Texture,
    size: (usize, usize),
}

#[derive(Script, ScriptHook, Widget)]
pub struct RenderedScene {
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
    draw_color: DrawSdfScene,
    #[live]
    draw_depth: DrawSdfScene,
    #[live(960.0)]
    render_width: f32,
    #[live(540.0)]
    render_height: f32,
    /// Only renders while a host shows it.
    #[rust]
    pub active: bool,
    /// Vertical FOV the cloud views it with (degrees).
    #[rust(50.0)]
    pub fov_y: f32,
    #[rust]
    targets: Option<Targets>,
}

impl RenderedScene {
    /// The colour and linear-depth targets and their pixel size, created on
    /// first use. Valid as textures right away; filled from the next draw.
    pub fn targets(&mut self, cx: &mut Cx) -> (Texture, Texture, (usize, usize)) {
        let size = (
            self.render_width.max(16.0) as usize,
            self.render_height.max(16.0) as usize,
        );
        let targets = self.targets.get_or_insert_with(|| {
            let fixed = TextureSize::Fixed {
                width: size.0,
                height: size.1,
            };
            let color = Texture::new_with_format(
                cx,
                TextureFormat::RenderBGRAu8 {
                    size: fixed.clone(),
                    initial: true,
                },
            );
            let depth = Texture::new_with_format(
                cx,
                TextureFormat::RenderRf32 {
                    size: fixed,
                    initial: true,
                },
            );
            let color_pass = DrawPass::new(cx);
            color_pass.set_color_texture(cx, &color, DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 1.0)));
            let depth_pass = DrawPass::new(cx);
            depth_pass.set_color_texture(cx, &depth, DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)));
            Targets {
                color_pass,
                depth_pass,
                color_list: DrawList2d::new(cx),
                depth_list: DrawList2d::new(cx),
                color,
                depth,
                size,
            }
        });
        (targets.color.clone(), targets.depth.clone(), targets.size)
    }
}

impl Widget for RenderedScene {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.walk_turtle(walk);
        if !self.active {
            return DrawStep::done();
        }
        let (_, _, (w, h)) = self.targets(cx.cx);
        let time = cx.time() as f32;
        let tan_y = (self.fov_y.to_radians() * 0.5).tan();
        let tan_half = vec2f(tan_y * w as f32 / h as f32, tan_y);
        let rect = Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(w as f64, h as f64),
        };
        let targets = self.targets.as_mut().expect("targets created above");
        for (draw, pass, list, depth) in [
            (&mut self.draw_color, &targets.color_pass, &mut targets.color_list, 0.0),
            (&mut self.draw_depth, &targets.depth_pass, &mut targets.depth_list, 1.0),
        ] {
            draw.scene_time = time;
            draw.output_depth = depth;
            draw.tan_half = tan_half;
            cx.make_child_pass(pass);
            cx.begin_pass(pass, Some(1.0));
            pass.set_size(cx.cx, dvec2(w as f64, h as f64));
            list.begin_always(cx);
            draw.draw_abs(cx, rect);
            list.end(cx);
            cx.end_pass(pass);
        }
        DrawStep::done()
    }
}
