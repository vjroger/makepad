//! The skyline strip: one code-native quad that paints a night silhouette
//! band (mountains, cities with lit windows, treelines, ground) with a
//! transparent sky.
//!
//! The strip is an equirectangular band. `u` (0..1) is 360 degrees and wraps;
//! elevation above the horizon is `(horizon - v) * (H / W) * 2pi` radians, so
//! every size below is angular and independent of the render resolution.
//! Each layer hashes integer cell ids of a fixed period and uses periodic
//! noise, so the left and right edges meet exactly.
//!
//! Output is premultiplied: silhouettes are opaque and dark, lights and glow
//! are added on top and may exceed 1.0 (HDR).
use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.shader.*

    mod.widgets.DrawSkyline = set_type_default() do #(DrawSkyline::script_shader(vm)) {
        ..mod.draw.DrawQuad
        seed: 1.0
        horizon: 0.72
        mountain_amt: 1.0
        mountain_height: 0.10
        city_amt: 1.0
        city_height: 1.0
        city_density: 0.7
        tree_amt: 1.0
        tree_height: 0.012
        tree_conifer: 0.7
        lights: 3.0
        window_density: 0.45
        glow: 0.5
        haze: 0.7
        sil_color: vec4(0.004, 0.006, 0.012, 1.0)
        haze_color: vec4(0.045, 0.060, 0.100, 1.0)

        // ---- hashing: integer, so it is identical on every GPU ----
        hash: fn(x_in: u32) -> u32 {
            var x = x_in
            x = x ^ (x >> 16u)
            x = x * 2146121005u
            x = x ^ (x >> 15u)
            x = x * 2221713035u
            x = x ^ (x >> 16u)
            return x
        }
        hash2: fn(a: u32, b: u32) -> u32 {
            return self.hash(a ^ self.hash(b + 2654435769u))
        }
        // Hash of (cell id, stream) in 0..1, per layer seed.
        rnd: fn(id: float, stream: float, layer: float) -> float {
            let s = u32(self.seed * 131.0 + layer * 7919.0)
            let h = self.hash2(self.hash2(u32(id + 4096.0), u32(stream) + 17u), s)
            return f32(h >> 8u) * 0.000000059604644775390625
        }
        rnd3: fn(a: float, b: float, c: float, layer: float) -> float {
            return self.rnd(a * 311.0 + b * 7.0 + 5000.0, c, layer)
        }

        // Value noise on a ring of `per` lattice points, `x` in 0..per.
        pn: fn(x: float, per: float, layer: float) -> float {
            let i = floor(x)
            let f = x - i
            let w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0)
            var i1 = i + 1.0
            if i1 >= per { i1 = i1 - per }
            let a = self.rnd(i, 1.0, layer)
            let b = self.rnd(i1, 1.0, layer)
            return mix(a, b, w)
        }
        // Periodic fBm in 0..1. `base` is the lattice period of the first octave.
        fbm: fn(u: float, base: float, layer: float) -> float {
            var s = 0.0
            var a = 0.5
            var per = base
            for k in 0..5 {
                s = s + a * self.pn(u * per, per, layer + f32(k) * 3.0)
                a = a * 0.5
                per = per * 2.0
            }
            return s / 0.96875
        }
        cover: fn(d: float, px: float) -> float {
            return clamp(d / px + 0.5, 0.0, 1.0)
        }

        // Mountain ridge: elevation of the ridge line at `u`.
        ridge: fn(u: float, base: float, layer: float) -> float {
            let n = self.fbm(u, base, layer)
            let r = 1.0 - abs(2.0 * n - 1.0)
            return mix(n, r, 0.55)
        }

        // ---- cities ----
        // Returns (emission rgb, mask).
        city: fn(u: float, ang: float, px: float, cells: float, layer: float, hmax: float, wsz: float, lit: float) -> vec4 {
            let x = u * cells
            let ci = floor(x)
            let lx = x - ci
            let pxc = px * cells / 6.2831853
            let cell_ang = 6.2831853 / cells

            // Low frequency density: where the downtown cores are.
            let env_n = self.fbm(u, 3.0, layer + 50.0)
            let core = smoothstep(0.35, 0.8, env_n * (0.55 + 0.9 * self.city_density))

            let r_h = self.rnd(ci, 2.0, layer)
            let r_w = self.rnd(ci, 3.0, layer)
            let r_j = self.rnd(ci, 4.0, layer)
            let r_t = self.rnd(ci, 5.0, layer)
            let r_a = self.rnd(ci, 6.0, layer)
            let r_c = self.rnd(ci, 7.0, layer)
            let r_b = self.rnd(ci, 8.0, layer)

            // Some cells are low blocks, tall ones only appear in the core.
            let tall = pow(r_h, 2.4) * core
            var hh = hmax * (0.10 + 0.9 * tall) * (0.45 + 0.55 * core)
            hh = hh * (0.55 + 0.45 * self.city_density + 0.0)
            hh = hh * self.city_amt
            let w = 0.55 + 0.4 * r_w
            let left = (1.0 - w) * 0.5 * (0.6 + 0.8 * r_j)
            let right = left + w

            let h1 = hh * (0.70 + 0.2 * r_t)
            // body
            let mx = smoothstep(left - pxc, left + pxc, lx) * smoothstep(right + pxc, right - pxc, lx)
            var m = mx * self.cover(h1 - ang, px)
            // crown (setback)
            let cw = w * (0.38 + 0.3 * r_t)
            let cl = left + (w - cw) * (0.3 + 0.4 * r_j)
            let cr = cl + cw
            let mc = smoothstep(cl - pxc, cl + pxc, lx) * smoothstep(cr + pxc, cr - pxc, lx)
            let crown_on = step(0.35, r_c)
            m = max(m, mc * crown_on * self.cover(hh - ang, px))
            // antenna on the tall ones
            let ant_on = step(0.62, r_a) * step(0.45 * hmax, hh)
            let acx = cl + cw * 0.5
            let aw = max(pxc * 0.7, 0.012)
            let atop = hh * (1.10 + 0.25 * r_a)
            let ma = smoothstep(acx - aw - pxc, acx - aw, lx) * smoothstep(acx + aw + pxc, acx + aw, lx)
            m = max(m, ma * ant_on * self.cover(atop - ang, px) * step(0.0, ang))

            // ---- windows ----
            var em = vec3(0.0, 0.0, 0.0)
            if lit > 0.0 {
                // Which tier is this pixel on: crown or body.
                let in_crown = mc * crown_on * step(h1, ang)
                let wl = mix(left, cl, in_crown)
                let ww = mix(w, cw, in_crown)
                let nwx = max(2.0, floor(ww * cell_ang / wsz))
                let wxf = (lx - wl) / ww * nwx
                let wxi = floor(wxf)
                let wyf = ang / wsz
                let wyi = floor(wyf)
                let fx = wxf - wxi
                let fy = wyf - wyi
                let prob = clamp(self.window_density * (0.15 + 1.1 * r_b), 0.0, 1.0)
                let rw = self.rnd3(ci + wxi * 13.0, wyi, 9.0, layer)
                let on = step(rw, prob)
                // aa in window-cell units
                let aax = px / wsz
                let shape = smoothstep(0.18 - aax, 0.18 + aax, fx) * smoothstep(0.82 + aax, 0.82 - aax, fx) * smoothstep(0.2 - aax, 0.2 + aax, fy) * smoothstep(0.72 + aax, 0.72 - aax, fy)
                // Below ~2px a window is just a speck: blend to its average.
                let res = clamp((wsz / px - 1.5) / 2.0, 0.0, 1.0)
                let avg = 0.36
                let wpat = mix(avg, shape, res)
                let warm = vec3(1.0, 0.62, 0.28)
                let cool = vec3(0.62, 0.80, 1.0)
                let wc = mix(warm, cool, step(0.8, fract(rw * 91.7)))
                let bright = 0.5 + 0.9 * fract(rw * 37.3)
                let in_b = max(mx * self.cover(h1 - ang, px), mc * crown_on * self.cover(hh - ang, px))
                em = wc * (on * wpat * bright * self.lights * lit * in_b * step(0.0, ang))
                // top beacon
                let ax = (lx - acx) / max(pxc, 0.0001)
                let ay = (ang - atop) / px
                let beacon = exp(-(ax * ax + ay * ay) / 3.0) * ant_on
                em = em + vec3(1.0, 0.08, 0.03) * (beacon * self.lights * 0.8 * lit * step(0.0, hh))
            }
            return vec4(em.x, em.y, em.z, m)
        }

        // ---- trees: two overlapping rows of conifers / round crowns ----
        trees: fn(u: float, ang: float, px: float, cells: float, layer: float, hmax: float) -> float {
            var best = 0.0
            for row in 0..2 {
                let off = 0.5 * f32(row)
                let x = u * cells + off
                var ci = floor(x)
                let lx = x - ci
                if ci >= cells { ci = ci - cells }
                let r_h = self.rnd(ci, 2.0, layer + f32(row) * 11.0)
                let r_s = self.rnd(ci, 3.0, layer + f32(row) * 11.0)
                let env = 0.45 + 0.55 * self.fbm(u, 7.0, layer + 90.0)
                let ht = hmax * env * (0.55 + 0.45 * r_h) * (1.0 - 0.18 * f32(row))
                let d = abs(2.0 * lx - 1.0)
                let spire = ht * pow(max(1.0 - d, 0.0), 0.85)
                let round = ht * 0.8 * sqrt(max(1.0 - d * d, 0.0)) * (0.85 + 0.15 * r_s)
                let prof = mix(round, spire, step(r_s, self.tree_conifer))
                best = max(best, self.cover(prof - ang, px))
            }
            return best
        }

        pixel: fn() {
            let u = self.pos.x
            let v = self.pos.y
            let ar = self.rect_size.y / self.rect_size.x
            let ang = (self.horizon - v) * ar * 6.2831853
            let px = ar * 6.2831853 / self.rect_size.y

            var col = vec3(0.0, 0.0, 0.0)
            var a = 0.0

            let sil = vec3(self.sil_color.x, self.sil_color.y, self.sil_color.z)
            let hz = vec3(self.haze_color.x, self.haze_color.y, self.haze_color.z)

            // City glow dome, additive, behind everything.
            let gcore = smoothstep(0.35, 0.8, self.fbm(u, 3.0, 50.0) * (0.55 + 0.9 * self.city_density))
            let dome = exp(-max(ang, 0.0) / (0.05 * self.city_height)) * gcore * self.glow * self.city_amt
            col = col + vec3(1.0, 0.55, 0.25) * (dome * 0.10)

            // Mountains, three ridges far -> near.
            for k in 0..3 {
                let t = (2.0 - f32(k)) / 2.0
                let hrel = 0.45 + 0.55 * f32(k) / 2.0
                let hr = self.mountain_amt * self.mountain_height * (0.35 + 0.65 * self.ridge(u, 3.0 + f32(k) * 2.0, 20.0 + f32(k) * 10.0)) * hrel
                let m = self.cover(hr - ang, px)
                let c = mix(sil, hz, t * self.haze * 0.9 + 0.1)
                col = mix(col, c, m)
                a = max(a, m)
            }

            // Cities: far, mid, near. Windows get larger and brighter closer up.
            let cfar = self.city(u, ang, px, 360.0, 100.0, 0.035 * self.city_height, 0.0016, 1.0)
            var cc = mix(sil, hz, 0.55 * self.haze + 0.1)
            col = mix(col, cc, cfar.w) + vec3(cfar.x, cfar.y, cfar.z) * 0.8
            a = max(a, cfar.w)

            let cmid = self.city(u, ang, px, 200.0, 200.0, 0.070 * self.city_height, 0.0024, 1.0)
            cc = mix(sil, hz, 0.28 * self.haze + 0.05)
            col = mix(col, cc, cmid.w) + vec3(cmid.x, cmid.y, cmid.z)
            a = max(a, cmid.w)

            let cnear = self.city(u, ang, px, 110.0, 300.0, 0.115 * self.city_height, 0.0034, 1.0)
            cc = mix(sil, hz, 0.06 * self.haze)
            col = mix(col, cc, cnear.w) + vec3(cnear.x, cnear.y, cnear.z)
            a = max(a, cnear.w)

            // Treeline in front of the city.
            let tm = self.trees(u, ang, px, 520.0, 400.0, self.tree_height * self.tree_amt)
            col = mix(col, sil, tm)
            a = max(a, tm)

            // Ground.
            let gh = 0.0035 + 0.0025 * self.fbm(u, 24.0, 500.0)
            let gm = self.cover(gh - ang, px)
            col = mix(col, sil * 0.6, gm)
            a = max(a, gm)

            return vec4(col.x, col.y, col.z, a)
        }
    }

    mod.widgets.SkylineViewBase = #(SkylineView::register_widget(vm))
    mod.widgets.SkylineView = set_type_default() do mod.widgets.SkylineViewBase {
        width: Fill height: Fill
        draw_bg: mod.widgets.DrawSkyline {}
    }
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawSkyline {
    #[deref]
    draw_super: DrawQuad,
    #[live(1.0)]
    pub seed: f32,
    #[live(0.72)]
    pub horizon: f32,
    #[live(1.0)]
    pub mountain_amt: f32,
    #[live(0.10)]
    pub mountain_height: f32,
    #[live(1.0)]
    pub city_amt: f32,
    #[live(1.0)]
    pub city_height: f32,
    #[live(0.7)]
    pub city_density: f32,
    #[live(1.0)]
    pub tree_amt: f32,
    #[live(0.012)]
    pub tree_height: f32,
    #[live(0.7)]
    pub tree_conifer: f32,
    #[live(3.0)]
    pub lights: f32,
    #[live(0.45)]
    pub window_density: f32,
    #[live(0.5)]
    pub glow: f32,
    #[live(0.7)]
    pub haze: f32,
    #[live]
    pub sil_color: Vec4f,
    #[live]
    pub haze_color: Vec4f,
}

#[derive(Script, ScriptHook, Widget)]
pub struct SkylineView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[redraw]
    #[live]
    pub draw_bg: DrawSkyline,
}

impl Widget for SkylineView {
    fn handle_event(&mut self, _cx: &mut Cx, _event: &Event, _scope: &mut Scope) {}

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        self.draw_bg.draw_walk(cx, walk);
        DrawStep::done()
    }
}

impl SkylineView {
    /// Set one parameter by name; returns false for an unknown name.
    pub fn set_param(&mut self, name: &str, v: f32) -> bool {
        let d = &mut self.draw_bg;
        match name {
            "seed" => d.seed = v,
            "horizon" => d.horizon = v,
            "mountain_amt" => d.mountain_amt = v,
            "mountain_height" => d.mountain_height = v,
            "city_amt" => d.city_amt = v,
            "city_height" => d.city_height = v,
            "city_density" => d.city_density = v,
            "tree_amt" => d.tree_amt = v,
            "tree_height" => d.tree_height = v,
            "tree_conifer" => d.tree_conifer = v,
            "lights" => d.lights = v,
            "window_density" => d.window_density = v,
            "glow" => d.glow = v,
            "haze" => d.haze = v,
            _ => return false,
        }
        true
    }
}

impl SkylineViewRef {
    pub fn set_param(&self, cx: &mut Cx, name: &str, v: f32) {
        if let Some(mut inner) = self.borrow_mut() {
            if inner.set_param(name, v) {
                inner.redraw(cx);
            }
        }
    }
}
