# Porting notes: Makepad (Rust host + shader march)

The crate `skyline-core` is already the host side: tiles → mosaics → tiers → footprints → plate. In a
Makepad app the only new piece is the march as a shader in a render-to-texture pass, with the CPU
`sweep` kept as the fallback and as the test oracle (the two must produce the same plate; the JS/GLSL
pair does today, see `test/gpu.test.mjs`).

## What moves where

| JS / GLSL today | Makepad |
| --- | --- |
| `tiles.js` fetch + `createImageBitmap` | `skyline_core::http::HttpTileSource` (sockets / curl) or Makepad `HttpRequest`; decode with `skyline_core::png::decode_png` |
| `buildings.js` Overpass POST | `HttpBuildingSource`, or `HttpRequest` + `skyline_core::json` |
| `tiers.js` / `buildings.js` rasterisation | `skyline_core::tiers`, `skyline_core::buildings` (unchanged) |
| tier textures `R32F` + `R8` | `Texture` per tier, format `R32f` (height) and `R8u`/`R32f` (class); 6 textures, 1024²–2048² |
| `FRAG_HORIZON` W × bands pre-pass | a `Pass` with a W × bands float colour texture, one quad draw |
| `FRAG_MARCH` strips, MRT float | a `Pass` with two float colour textures (depth/alt/class/coverage and normal), drawn in strips |
| `readPixels` | read the pass textures back to CPU (verify per backend: Metal, DX11, OpenGL, WebGL) |
| `export/*.js` | `skyline_core::export` (unchanged) |

## The shader, piece by piece

Everything in `src/gpu/shaders.js` is plain arithmetic plus `texelFetch`. The parts to carry over, in
order: `sampleTier` (bilinear with explicit clamping), `heightAt` (vertical exaggeration), `tierFor`,
`stepSize`, `normalAt`, the pre-pass loop (max apparent elevation per column up to a radius), and the
per-pixel loop (ray height `z = eye + d·tan(e) + d²/(2R_eff)`, hit when `h(d) ≥ z`, 6 bisection steps).

Sketch of the per-pixel march in Makepad's shader language (untested, structure only — check the current
MPSL syntax for texture sampling, uniform arrays and loops with `break` before relying on it):

```rust
// live_design! { ... draw_march: { ... } }
fn height_at(self, k: int, en: vec2) -> float {
    // bilinear fetch from the k-th tier texture; mirror sampleTier() exactly:
    // fi = en.x / texel + size*0.5 - 0.5, i0 = floor(fi), clamp to [0, size-1], mix the four texels
}
fn pixel(self) -> vec4 {
    let theta = (self.pos.x) * 6.283185 - 3.141593 + self.heading;      // self.pos in 0..1
    let e = (self.pos.y - 0.5) * 3.141593;                                // bottom = -90 deg
    let dir = vec2(sin(theta), cos(theta));
    let tan_e = tan(e);
    let mut d = 1.0; let mut d_prev = 0.0; let mut hit = 0.0;
    for i in 0..24000 {
        let en = d * dir;
        let k = self.tier_for(en); if k < 0 { break; }
        let h = self.height_at(k, en);
        let z_ray = self.eye_z + d * tan_e + d * d * self.inv_r2;
        if h >= z_ray { hit = 1.0; break; }
        if z_ray > self.h_max && tan_e + 2.0 * d * self.inv_r2 >= 0.0 { break; }
        if d >= self.max_dist { break; }
        d_prev = d; d = d + max(0.5 * self.texel(k), 0.5 * d * self.delta);
    }
    // bisection between d_prev and d, then depth / class / normal as in FRAG_MARCH
    return vec4(depth, altitude, class, hit);
}
```

If MPSL does not allow a data-dependent `break`, run the loop to a fixed count with a `done` flag
(same cost on a GPU, the lanes idle anyway), or move the pre-pass horizon test outside the loop.

## Things to confirm early

1. Float colour textures as render targets and reading them back on every backend you ship.
2. Six 2048² float textures (85 MB) are fine on desktop GPUs; on mobile, drop tier 5 and use 1024² far tiers.
3. Draw in strips (the WebGL renderer uses ~2^20 pixels per draw) so no single draw trips a GPU watchdog.
4. HTTP from both native and WASM builds; the WASM build fetches the same keyless S3 tiles the browser prototype does.
5. If readback is a problem on one backend, `skyline_core::render` gives the identical plate on the CPU in well under a second at 4096 width on 8 cores.
