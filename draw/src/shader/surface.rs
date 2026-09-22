//! The surface material: bevel, emboss, inner shadow, contact occlusion, rim,
//! specular and gloss, computed analytically from a shape's own signed
//! distance, in the pass that is already running.
//!
//! # Why the distance is enough
//!
//! A widget's `pixel` function already holds an exact signed distance to its
//! own boundary. `vec2(dFdx(d), dFdy(d))` is that field's gradient — unit
//! length wherever the field is well formed — for two instructions. Run the
//! inside depth through a height profile, tilt the gradient by that profile's
//! slope, and the result is a surface normal. One Lambert and one Blinn-Phong
//! term against a light direction shared by the whole theme then produce a
//! moulded edge that agrees with every other control on screen, which is what
//! makes a skeuomorphic panel cohere.
//!
//! No extra pass, no texture, no bandwidth, and nothing at all when a theme
//! leaves it off: `shade` takes a `level` that callers drive from a UNIFORM, so
//! the branch is draw-call coherent and never diverges inside a warp.
//!
//! # Why this is not a method on Sdf2d
//!
//! It must not add fields to `Sdf2d`. The gpusim JIT skips that struct's
//! definition and takes it from `os/gpusim/shader_runtime_preamble.rs`, while
//! still emitting the Splash method bodies — so a new METHOD translates but a
//! new FIELD would silently diverge from the preamble's layout. Every function
//! here therefore takes a plain `float` distance and returns a plain value;
//! nothing is threaded through the sdf at all.
//!
//! The internal `*_of` helpers are plain `fn`s called bare, and the exported
//! names wrap them — the shape `GaussShadow` uses in `sdf.rs` for `gaussian`
//! and `rounded_box_shadow_x`. Widgets reach the exported ones as
//! `Material.shade(...)`, because `widgets/src/lib.rs` splices `..mod.sdf`
//! into the prelude.
//!
//! # Perf law
//!
//! Nothing here reads `draw_pass.time`. A shader that does is flagged
//! `uses_time` and pins the whole window at display rate for as long as it is
//! on screen (see the note in `widgets/src/gauss_view.rs`). Dither is hashed
//! from screen position only.
pub use crate::makepad_platform::*;

script_mod! {
    use mod.pod.*
    use mod.math.*
    use mod.sdf

    mod.sdf.Material = {
        // The outward normal of the field in screen space. Unit length
        // wherever the field is well formed; the guard keeps a flat interior,
        // where the gradient is zero, from producing a NaN.
        fn grad_of(d: float) -> vec2 {
            let g = vec2(dFdx(d), dFdy(d));
            if length(g) > 0.00001 {
                return normalize(g);
            }
            return vec2(0.0, 1.0);
        }

        // The relief's height over the inside depth: 0 at the boundary, 1 at
        // `width` points in. `curve` picks the profile — 0 is a smoothstep
        // ramp, flat at the edge and steep in the middle, which is the soft
        // pillowed neumorphic shoulder; 1 is a quarter circle, steep at the
        // edge and flat in the middle, which is moulded plastic.
        fn height_of(d: float, width: float, curve: float) -> float {
            let t = clamp(-d / max(width, 0.001), 0.0, 1.0);
            let soft = t * t * (3.0 - 2.0 * t);
            let round = sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
            return mix(soft, round, clamp(curve, 0.0, 1.0));
        }

        // dh/dt of that profile: what actually tilts the normal. The round
        // profile is vertical at the boundary, so its slope is capped rather
        // than left to run to infinity and alias.
        fn slope_of(d: float, width: float, curve: float) -> float {
            let t = clamp(-d / max(width, 0.001), 0.0, 1.0);
            let omt = 1.0 - t;
            let soft = 6.0 * t * omt;
            let round = omt / max(sqrt(max(1.0 - omt * omt, 0.0)), 0.125);
            return min(mix(soft, round, clamp(curve, 0.0, 1.0)), 8.0);
        }

        // The surface normal of the relief. `elev` is SIGNED: positive is a
        // raised face, negative a sunken one, and that sign alone swaps which
        // edge is lit — the whole of the raised/sunken and pressed/unpressed
        // trick lives here.
        fn normal_of(d: float, width: float, curve: float, elev: float) -> vec3 {
            let g = grad_of(d);
            let s = slope_of(d, width, curve) * elev;
            return normalize(vec3(g.x * s, g.y * s, 1.0));
        }

        // Lambert in .x, Blinn-Phong in .y. The view direction is straight out
        // of the screen, so the half vector is against vec3(0, 0, 1).
        fn lit_of(n: vec3, light: vec4, spec: vec2) -> vec2 {
            let l = normalize(light.xyz);
            let diffuse = max(dot(n, l), 0.0) * light.w;
            let h = normalize(l + vec3(0.0, 0.0, 1.0));
            let s = pow(max(dot(n, h), 0.0), max(spec.y, 1.0)) * spec.x;
            return vec2(diffuse, s);
        }

        // What a FLAT face reads under this light. Subtracting it is what
        // keeps the middle of a control its own colour and confines the
        // lighting to the shoulders, instead of tinting the whole face.
        fn flat_of(light: vec4) -> float {
            return max(normalize(light.xyz).z, 0.0) * light.w;
        }

        // The field as it would read `offset` points away, to first order.
        // Two multiply-adds instead of re-evaluating the shape, which is what
        // makes an offset inner shadow cheap enough to always have on.
        fn shift_of(d: float, offset: vec2) -> float {
            return d - dot(grad_of(d), offset);
        }

        // Contact darkening that hugs the whole inside edge with no direction
        // to it. This is what makes a well read as sunken rather than merely
        // outlined.
        fn ao_of(d: float, radius: float) -> float {
            return (1.0 - smoothstep(0.0, max(radius, 0.001), -d)) * step(d, 0.0);
        }

        // The lit edge band of a raised face, one `width` in from the boundary.
        fn rim_of(d: float, n: vec3, light: vec4, width: float) -> float {
            let facing = max(dot(n, normalize(light.xyz)), 0.0);
            return (1.0 - smoothstep(0.0, max(width, 0.001), -d)) * pow(facing, 0.8) * step(d, 0.0);
        }

        // The sweep across the top of a glossy cap. `uv` is the rect-local
        // 0..1 position, so this is in the face's own space, not the screen's.
        fn gloss_of(uv: vec2, top: float, power: float) -> float {
            return pow(clamp(1.0 - uv.y / max(top, 0.001), 0.0, 1.0), max(power, 0.001));
        }

        // ---- the exported surface ----

        grad: fn(d: float) -> vec2 {
            return grad_of(d);
        }

        height: fn(d: float, width: float, curve: float) -> float {
            return height_of(d, width, curve);
        }

        slope: fn(d: float, width: float, curve: float) -> float {
            return slope_of(d, width, curve);
        }

        normal: fn(d: float, width: float, curve: float, elev: float) -> vec3 {
            return normal_of(d, width, curve, elev);
        }

        lit: fn(n: vec3, light: vec4, spec: vec2) -> vec2 {
            return lit_of(n, light, spec);
        }

        shift: fn(d: float, offset: vec2) -> float {
            return shift_of(d, offset);
        }

        // A directional inner shadow: 1 along the edge the light does not
        // reach, falling to 0 `radius` points inside.
        inner: fn(d: float, offset: vec2, radius: float) -> float {
            let s = shift_of(d, offset);
            return (1.0 - smoothstep(0.0, max(radius, 0.001), -s)) * step(d, 0.0);
        }

        ao: fn(d: float, radius: float) -> float {
            return ao_of(d, radius);
        }

        rim: fn(d: float, n: vec3, light: vec4, width: float) -> float {
            return rim_of(d, n, light, width);
        }

        gloss: fn(uv: vec2, top: float, power: float) -> float {
            return gloss_of(uv, top, power);
        }

        // An emissive halo OUTSIDE the shape. The caller has to have left room
        // for it in its quad, which the material widgets do by insetting their
        // shape rather than growing their geometry.
        glow: fn(d: float, radius: float) -> float {
            return exp(-max(d, 0.0) / max(radius, 0.001));
        }

        // The one call a retrofitted widget makes, between its fill colour and
        // `fill_keep`.
        //
        //   level    0 off (returns rgb untouched), 1 relief, 2 full
        //   light    xyz direction in UI space (x right, y down, z out), w intensity
        //   relief   bevel width, profile curve, SIGNED elevation, specular strength
        //   finish   ao, rim, gloss, roughness
        //
        // `level` is read from a uniform by every caller, so the two early
        // returns cost one scalar compare per draw call and nothing per pixel.
        shade: fn(
            rgb: vec3,
            d: float,
            uv: vec2,
            level: float,
            light: vec4,
            relief: vec4,
            finish: vec4,
            light_ink: vec4,
            shadow_ink: vec4
        ) -> vec3 {
            if level < 0.5 {
                return rgb;
            }
            let n = normal_of(d, relief.x, relief.y, relief.z);
            let spec = vec2(relief.w, mix(64.0, 4.0, clamp(finish.w, 0.0, 1.0)));
            let lt = lit_of(n, light, spec);

            // Signed deviation from a flat face: positive where the shoulder
            // turns into the light, negative where it turns away.
            let key = (lt.x - flat_of(light)) * 1.5;
            let lift = clamp(key, 0.0, 1.0) * light_ink.a;
            let drop = clamp(-key, 0.0, 1.0) * shadow_ink.a;
            var out = mix(rgb, light_ink.rgb, lift);
            out = mix(out, shadow_ink.rgb, drop);

            // Contact occlusion over twice the bevel, so it reaches past the
            // shoulder and reads as the surface meeting its ground.
            let occ = ao_of(d, relief.x * 2.0) * finish.x * shadow_ink.a;
            out = mix(out, shadow_ink.rgb, occ);

            if level < 1.5 {
                return out;
            }

            let r = rim_of(d, n, light, max(relief.x * 0.5, 0.5)) * finish.y * light_ink.a;
            out = mix(out, light_ink.rgb, clamp(r, 0.0, 1.0));
            out = out + vec3(lt.y);
            let g = gloss_of(uv, 0.55, 2.0) * finish.z * step(d, 0.0) * light_ink.a;
            out = mix(out, light_ink.rgb, clamp(g, 0.0, 1.0));
            return out;
        }
    }
}
