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
        fn normal_of(g: vec2, d: float, width: float, curve: float, elev: float) -> vec3 {
            let s = slope_of(d, width, curve) * elev;
            return normalize(vec3(g.x * s, g.y * s, 1.0));
        }

        // A wrapped Lambert. The terminator on a steep shoulder is a soft
        // roll-off, not a hard line at n.l = 0; `flat_of` goes through the
        // same curve so a flat face still reads as exactly flat.
        fn lam_of(ndl: float) -> float {
            return smoothstep(-0.35, 1.0, ndl);
        }

        // Lambert in .x, Blinn-Phong in .y. The view direction is straight out
        // of the screen, so the half vector is against vec3(0, 0, 1).
        fn lit_of(n: vec3, light: vec4, spec: vec2) -> vec2 {
            let l = normalize(light.xyz);
            let diffuse = lam_of(dot(n, l)) * light.w;
            let h = normalize(l + vec3(0.0, 0.0, 1.0));
            let s = pow(max(dot(n, h), 0.0), max(spec.y, 1.0)) * spec.x;
            return vec2(diffuse, s);
        }

        // What a FLAT face reads under this light. Subtracting it is what
        // keeps the middle of a control its own colour and confines the
        // lighting to the shoulders, instead of tinting the whole face.
        fn flat_of(light: vec4) -> float {
            return lam_of(normalize(light.xyz).z) * light.w;
        }

        // The direction a shadow falls: away from the light, flattened into
        // the plane. Degenerate when the light is straight on, so it falls
        // back to down-screen, which is what a UI convention expects anyway.
        fn shadow_dir_of(light: vec4) -> vec2 {
            let len = length(light.xy);
            if len > 0.0001 {
                return -light.xy / len;
            }
            return vec2(0.0, 1.0);
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

        // The sheen on a glossy cap is a REFLECTION, so it is read off the
        // normal, not off position: a soft light box above and in front, with
        // the flat face's share subtracted so a flat top keeps its own colour
        // and only surfaces that tilt toward the sky pick it up -- domes and
        // rims. A ramp over uv.y did the same on a round cap and painted the
        // whole arm of a pointer knob white, because the arm was the top of
        // the bounding box.
        fn gloss_of(n: vec3) -> float {
            let sky = normalize(vec3(0.0, -0.6, 0.8));
            let s = max(dot(n, sky), 0.0);
            return clamp((s * s - 0.64) * 2.8, 0.0, 1.0);
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
            return normal_of(grad_of(d), d, width, curve, elev);
        }

        lit: fn(n: vec3, light: vec4, spec: vec2) -> vec2 {
            return lit_of(n, light, spec);
        }

        shift: fn(d: float, offset: vec2) -> float {
            return shift_of(d, offset);
        }

        // A directional inner shadow: 1 along the edge the light does not
        // reach, falling to 0 `radius` points inside.
        //
        // # Do not use this on a rectangle
        //
        // It is a falloff over the DISTANCE FIELD, and the interior field of a
        // rounded rectangle has a ridge along its corner diagonals — its
        // medial axis, where the nearest edge switches from one side to the
        // other. Anything shaped as `smoothstep(distance)` creases along that
        // ridge, and the wider the radius the further inward the crease runs,
        // which shows as hard bright spikes reaching in from the corners. The
        // ridge is a true property of the field, so no better rounded-box
        // formula removes it: the shape of the falloff is what is wrong.
        //
        // A convolution has no such feature, because it integrates over the
        // shape instead of reading one nearest point — so a box passes
        // `inner_cov` below to `shade` instead. This form stays correct for a
        // CIRCLE, whose medial axis is a single point at its centre, and for
        // the cast shadow, which only ever reads the field outside the shape
        // where a convex shape has no medial axis at all.
        inner: fn(d: float, offset: vec2, radius: float) -> float {
            let s = shift_of(d, offset);
            return (1.0 - smoothstep(0.0, max(radius, 0.001), -s)) * step(d, 0.0);
        }

        // The inner shadow of a BOX, as a real blurred coverage.
        //
        // The lit part of a dropped face is its own outline shifted DOWN-LIGHT
        // — light passing over the rim lands offset — so the shadow is
        // everything that shifted outline does not cover. `GaussShadow`
        // already integrates exactly that, which is why the fix is to reuse it
        // rather than to sharpen the falloff above.
        //
        //   lower/upper  the face's own rect, in the same space as `point`
        //   depth        how far it sits BELOW its surround, positive
        inner_cov: fn(
            lower: vec2,
            upper: vec2,
            point: vec2,
            corner: float,
            radius: float,
            depth: float,
            light: vec4
        ) -> float {
            let off = shadow_dir_of(light) * depth * 1.6;
            let cov = GaussShadow.rounded_box_shadow(
                lower + off,
                upper + off,
                point,
                max(radius * 0.5, 0.35),
                corner
            );
            return 1.0 - cov;
        }

        ao: fn(d: float, radius: float) -> float {
            return ao_of(d, radius);
        }

        rim: fn(d: float, n: vec3, light: vec4, width: float) -> float {
            return rim_of(d, n, light, width);
        }

        gloss: fn(n: vec3) -> float {
            return gloss_of(n);
        }

        // An emissive halo OUTSIDE the shape. The caller has to have left room
        // for it in its quad, which the material widgets do by insetting their
        // shape rather than growing their geometry.
        glow: fn(d: float, radius: float) -> float {
            return exp(-max(d, 0.0) / max(radius, 0.001));
        }

        // Everything a raised face throws OUTSIDE itself, premultiplied and
        // ready for `sdf.clear` before the shape is filled — the same place
        // and the same idiom as `GaussShadow.rounded_box_shadow` in
        // `RoundedShadowView`.
        //
        // `shade` colours the inside of a face and nothing else, so on its own
        // a raised control cast nothing onto its ground and read flat however
        // well its shoulders were lit. Outside the face there is no fill to
        // shade, so the shadow is a second read of the SAME field: the
        // distance to the shape translated away from the light is exactly the
        // distance to where its shadow falls. That is `shift_of` — two
        // multiply-adds, no second evaluation of the shape.
        //
        //   depth    how far the face stands off its surround, in points
        //   raise    the theme's reference elevation, which `depth` is read against
        //   shadow   cast strength, cast blur, contact occlusion, ground lip
        //
        // GROUND LIP is the light-side counterpart, and it is deliberately its
        // own control rather than welded to the cast shadow. It only means
        // anything where the control is EXTRUDED FROM the page — one
        // continuous surface, so the lit side is the ground bending up into
        // it. A control resting ON a panel is a separate object and casts a
        // dark shadow only; a lip there reads as a button emitting light for
        // no reason. Light that genuinely leaves a control is `glow`.
        cast: fn(
            d: float,
            depth: float,
            raise: float,
            light: vec4,
            shadow: vec4,
            shadow_ink: vec4,
            light_ink: vec4
        ) -> vec4 {
            // Only a face standing proud of its surround casts anything, and
            // only outside itself. A pressed or sunken face returns nothing
            // here and earns its inner shadow in `shade` instead.
            if depth <= 0.0 {
                return vec4(0.0);
            }
            // A soft gate, not `step(0.0, d)`. The caller composites its fill
            // over this with an antialiased edge about a pixel wide, so the
            // shadow must reach that far inside the shape too; a hard gate
            // leaves bare ground under the edge pixels, and the fill blends
            // with it into a one-pixel light line around every shadowed edge.
            let px = length(vec2(dFdx(d), dFdy(d)));
            let outside = smoothstep(-3.0 * max(px, 0.001), 0.0, d);
            let rel = clamp(depth / max(raise, 0.001), 0.0, 1.0);
            let g = grad_of(d);
            // The shadow is thrown by a HEIGHT under a LIGHT: off = H / tan(el).
            // A low light throws it far; a high one keeps it under the object.
            let sdir = shadow_dir_of(light);
            let tanel = max(light.z, 0.05) / max(length(light.xy), 0.05);
            let off = sdir * (depth / tanel);
            let blur = max(shadow.y, 0.001);
            // The lip is the ground curving up into a LIT shoulder, so it exists
            // only where the outward normal faces the light. Painted all round,
            // it landed on the shadow side too and pushed the shadow off the
            // edge, which is what made a raised control look as if it floated.
            let facing = smoothstep(0.0, 0.7, clamp(-dot(g, sdir), 0.0, 1.0));

            // shift_of(d, off) and shift_of(d, -off), with the one gradient
            // shared rather than taken twice.
            let dark = exp(-max(d - dot(g, off), 0.0) / blur) * rel;
            let lite = exp(-max(d + dot(g, off), 0.0) / blur) * rel * facing;
            // Contact darkening is much tighter than the cast shadow: it is
            // what grounds a raised cap, and it is the right answer for a
            // raised face where self-occlusion is not.
            let contact = exp(-max(d, 0.0) / (blur * 0.3));

            let a_dark = clamp(dark * shadow.x + contact * shadow.z, 0.0, 1.0) * shadow_ink.a * outside;
            let a_lite = clamp(lite * shadow.w, 0.0, 1.0) * light_ink.a * outside * (1.0 - a_dark);
            let rgb = shadow_ink.rgb * a_dark + light_ink.rgb * a_lite;
            return vec4(rgb, a_dark + a_lite);
        }

        // The one call a retrofitted widget makes, between its fill colour and
        // `fill_keep`.
        //
        //   level    0 off (returns rgb untouched), 1 relief, 2 full
        //   light    xyz direction in UI space (x right, y down, z out), w intensity
        //   relief   bevel width, profile curve, SIGNED CONVEXITY, specular strength
        //   finish   ao, rim, gloss, roughness
        //   form       SIGNED DEPTH, face gradient, hairline, occlusion reach
        //   deep       inner shadow, inner radius, sink reference, raise reference
        //   inner_cov  the inner shadow's coverage, from `inner_cov` or `inner`
        //
        // # Depth and convexity are two different numbers
        //
        // They used to be one, and that is why a press had to flip its sign —
        // which turns a cap into a bowl. DEPTH is how far a face sits from its
        // surround, and it drives the cast shadow outside and the inner shadow
        // inside. CONVEXITY is whether the face itself bulges out or dishes
        // in, and it drives the normal, the gradient, the hairline and the
        // self-occlusion. A pressed cap is still a cap: it has descended, so
        // its own shadow is gone and its surround throws one across it, but
        // its face never dishes. Full inversion is the neumorphic illustration
        // convention and is something the caller opts into by passing a
        // negative convexity, not something the material does on its own.
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
            form: vec4,
            deep: vec4,
            inner_cov: float,
            light_ink: vec4,
            shadow_ink: vec4
        ) -> vec3 {
            if level < 0.5 {
                return rgb;
            }
            let convex = relief.z;
            let depth = form.x;
            let sink = max(deep.z, 0.001);
            let g = grad_of(d);
            let n = normal_of(g, d, relief.x, relief.y, convex);
            let spec = vec2(relief.w, mix(64.0, 4.0, clamp(finish.w, 0.0, 1.0)));
            let lt = lit_of(n, light, spec);

            // Signed deviation from a flat face: positive where the shoulder
            // turns into the light, negative where it turns away.
            // TWO RAMPS THAT MEET FLAT. clamp() gave each side a straight ramp
            // from zero, and the shadow ink sits about four times further from
            // the base than white does, so the slopes differed and every dome
            // carried a V-shaped crease where they met. smoothstep starts with
            // zero slope on both sides.
            let key = (lt.x - flat_of(light)) * 1.5;
            let lift = smoothstep(0.0, 1.0, key) * light_ink.a;
            let drop = smoothstep(0.0, 1.0, -key) * shadow_ink.a;
            var out = mix(rgb, light_ink.rgb, lift);
            out = mix(out, shadow_ink.rgb, drop);

            // THE FACE GRADIENT. The bevel normal is flat everywhere but
            // within `relief.x` of the edge, so without this the middle of
            // every face is exactly its own fill colour — which is why
            // shoulders alone never looked like moulded plastic however they
            // were tuned. A real face is curved across its whole span, and
            // every reference kit carries this broad gradient through it.
            // Makepad already does the same thing by hand through
            // `color` -> `color_2` fills, so this is joining up machinery that
            // exists rather than inventing more.
            if form.y > 0.001 {
                // Toward the light -- `axis` points AT the light, so the lit
                // side is where dot is positive. One smooth target with a C1
                // weight: two clamped ramps met at zero in a visible fold.
                let axis = normalize(light.xy + vec2(0.000001));
                let t = dot(uv - vec2(0.5), axis) * 2.0 * sign(convex);
                let gink = mix(shadow_ink.rgb, light_ink.rgb, vec3(smoothstep(-1.0, 1.0, t)));
                out = mix(out, gink, vec3(form.y * 0.5 * smoothstep(0.0, 1.0, abs(t))));
            }

            // OCCLUSION IS CONCAVITY. This used to darken the rim of every
            // face over twice the bevel, and the rim of a RAISED face is the
            // most exposed point on it — the top of the hill, with nothing
            // above it to block anything — so it fought the very shoulder the
            // bevel had just lit, on the same band of pixels. Only a face that
            // curves away from the viewer occludes itself at its rim. What
            // grounds a raised face is the contact shadow on the ground around
            // its base, which `cast` draws outside and is the right place for
            // it. The reach is in units of the shoulder now, not a hardcoded
            // double, so it cannot spill onto face the relief says is flat.
            let concave = clamp(-convex / sink, 0.0, 1.0);
            let occ = ao_of(d, max(relief.x, 0.001) * form.w) * finish.x * shadow_ink.a * concave;
            out = mix(out, shadow_ink.rgb, occ);

            // THE HAIRLINE. A hard thin line right on the boundary, lit on the
            // side facing the light and dark opposite — distinct from the soft
            // shoulder, and what gives the reference kits their crispness.
            // The stroke half of the same pair as the gradient above, which
            // makepad spells `border_color` -> `border_color_2`.
            if form.z > 0.001 {
                let band = 1.0 - smoothstep(0.0, 1.4, abs(d));
                let facing = dot(g, normalize(light.xy + vec2(0.000001))) * sign(convex);
                out = mix(out, light_ink.rgb, band * clamp(facing, 0.0, 1.0) * form.z * light_ink.a);
                out = mix(out, shadow_ink.rgb, band * clamp(-facing, 0.0, 1.0) * form.z * shadow_ink.a);
            }

            // THE INNER SHADOW, which nothing used to call at all — a sunken
            // well got the inverted bevel and the occlusion and nothing else.
            // Scaled by how sunken the face is, so a press earns it as it
            // crosses over rather than switching it on.
            //
            // `inner_cov` is what the CALLER passes in, because the honest
            // version of this is a blurred coverage of the face's own rect and
            // only the caller knows that rect. A box computes it with
            // `Material.inner_cov`; a circle may pass `Material.inner(...)`,
            // which is cheaper and correct for a shape whose medial axis is a
            // single point. Computing it here from the field gradient is what
            // put bright spikes along the corner diagonals of every pressed
            // rectangle — see the note on `inner`.
            let sunk = clamp(-depth / sink, 0.0, 1.0);
            if sunk > 0.001 {
                let ins = inner_cov * step(d, 0.0);
                out = mix(out, shadow_ink.rgb, clamp(ins * deep.x * sunk, 0.0, 1.0) * shadow_ink.a);
            }

            if level < 1.5 {
                return out;
            }

            let r = rim_of(d, n, light, max(relief.x * 0.5, 0.5)) * finish.y * light_ink.a;
            out = mix(out, light_ink.rgb, clamp(r, 0.0, 1.0));
            out = out + vec3(lt.y);
            let gl = gloss_of(n) * finish.z * step(d, 0.0) * light_ink.a;
            out = mix(out, light_ink.rgb, clamp(gl, 0.0, 1.0));
            return out;
        }
    }
}
