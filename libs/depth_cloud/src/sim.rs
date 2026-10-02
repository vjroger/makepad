//! Point physics for the mouse effects: every grid point carries an offset
//! from its rest position and a velocity. The effector pushes (attract,
//! repel, swirl, ripple), a spring pulls each point home and damping bleeds
//! the energy off, so points fly, overshoot and settle. The offsets go to
//! the shader as an RGBA f32 texture, one texel per point.
//!
//! Runs on the UI thread in `draw_3d` only while something moves: idle (no
//! effector, everything settled) it costs nothing and binds no texture.

use crate::cloud::CloudEffect;
use makepad_widgets::makepad_draw::*;

/// Where the points rest, to measure their distance to the effector.
pub(crate) enum RestDepth<'a> {
    /// The normalized disparity map of a frame source (1 = near, < 0 = none)
    /// and 1/far of the depth mapping.
    Map {
        width: usize,
        height: usize,
        values: &'a [f32],
        inv_far: f32,
    },
    /// Rendered sources: depth is on the GPU only; points rest on this plane.
    Plane(f32),
}

pub(crate) struct SimParams<'a> {
    pub cols: usize,
    pub rows: usize,
    pub rest: RestDepth<'a>,
    /// tan(fov/2) horizontally and vertically.
    pub tan_half: (f32, f32),
    pub effector: Option<Vec3f>,
    pub effect: CloudEffect,
    pub strength: f32,
    pub radius: f32,
    /// Spring stiffness (1/s^2) and damping (1/s).
    pub spring: f32,
    pub damping: f32,
    pub time: f64,
}

#[derive(Default)]
pub(crate) struct PointSim {
    cols: usize,
    rows: usize,
    offset: Vec<[f32; 3]>,
    velocity: Vec<[f32; 3]>,
    texture: Option<Texture>,
    last_time: Option<f64>,
    moving: bool,
}

/// Below this (offset + speed, cloud units) a point counts as at rest.
const REST_EPSILON: f32 = 1e-4;

impl PointSim {
    /// True while points are still away from home or moving.
    pub(crate) fn is_moving(&self) -> bool {
        self.moving
    }

    /// Advance to `params.time`; returns the offset texture while any point
    /// is displaced (else `None`: draw at rest, no texture needed).
    pub(crate) fn step(&mut self, cx: &mut Cx, params: &SimParams) -> Option<Texture> {
        let count = params.cols * params.rows;
        if (self.cols, self.rows) != (params.cols, params.rows) {
            self.cols = params.cols;
            self.rows = params.rows;
            self.offset = vec![[0.0; 3]; count];
            self.velocity = vec![[0.0; 3]; count];
            self.texture = None;
            self.moving = false;
        }
        let dt = match self.last_time {
            Some(last) => (params.time - last).clamp(0.0, 1.0 / 30.0) as f32,
            None => 1.0 / 60.0,
        };
        self.last_time = Some(params.time);
        let effector = params.effector.filter(|_| params.effect != CloudEffect::Off);
        if effector.is_none() && !self.moving {
            return None;
        }
        if dt <= 0.0 {
            return self.texture.clone().filter(|_| self.moving);
        }

        let radius = params.radius.max(1e-3);
        // Peak effector acceleration (cloud units / s^2) at full strength.
        let push = params.strength * 60.0 * radius;
        let (k, c) = (params.spring.max(0.0), params.damping.max(0.0));
        let substeps = 2;
        let h = dt / substeps as f32;
        let time = params.time as f32;
        let mut moving = false;
        for row in 0..params.rows {
            let v = (row as f32 + 0.5) / params.rows as f32;
            for col in 0..params.cols {
                let u = (col as f32 + 0.5) / params.cols as f32;
                let i = row * params.cols + col;
                let Some(z) = rest_z(&params.rest, u, v) else {
                    self.offset[i] = [0.0; 3];
                    self.velocity[i] = [0.0; 3];
                    continue;
                };
                let rest = [
                    (u * 2.0 - 1.0) * params.tan_half.0 * z,
                    (1.0 - v * 2.0) * params.tan_half.1 * z,
                    -z,
                ];
                let (mut o, mut vel) = (self.offset[i], self.velocity[i]);
                for _ in 0..substeps {
                    let mut f = [-k * o[0] - c * vel[0], -k * o[1] - c * vel[1], -k * o[2] - c * vel[2]];
                    if let Some(e) = effector {
                        let d = [
                            rest[0] + o[0] - e.x,
                            rest[1] + o[1] - e.y,
                            rest[2] + o[2] - e.z,
                        ];
                        let r2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                        let fall = (-r2 / (radius * radius)).exp();
                        if fall > 1e-4 {
                            let r = r2.sqrt().max(1e-4);
                            let s = push * fall;
                            let add = match params.effect {
                                CloudEffect::Attract => [-d[0] / r * s, -d[1] / r * s, -d[2] / r * s],
                                CloudEffect::Repel => [d[0] / r * s, d[1] / r * s, d[2] / r * s],
                                CloudEffect::Swirl => [-d[1] / r * s, d[0] / r * s, 0.0],
                                CloudEffect::Ripple => {
                                    [0.0, 0.0, (r * 12.0 / radius - time * 6.0).sin() * s]
                                }
                                CloudEffect::Off => [0.0; 3],
                            };
                            f = [f[0] + add[0], f[1] + add[1], f[2] + add[2]];
                        }
                    }
                    // Semi-implicit Euler: stable for a stiff spring.
                    for a in 0..3 {
                        vel[a] += f[a] * h;
                        o[a] += vel[a] * h;
                    }
                }
                let energy = o[0].abs() + o[1].abs() + o[2].abs() + (vel[0].abs() + vel[1].abs() + vel[2].abs()) * h;
                if energy > REST_EPSILON {
                    moving = true;
                } else if effector.is_none() {
                    o = [0.0; 3];
                    vel = [0.0; 3];
                }
                self.offset[i] = o;
                self.velocity[i] = vel;
            }
        }
        self.moving = moving || effector.is_some();
        if !self.moving {
            return None;
        }

        let mut data = match &self.texture {
            Some(texture) => texture.take_vec_f32(cx),
            None => Vec::new(),
        };
        data.clear();
        data.reserve(count * 4);
        for o in &self.offset {
            data.extend_from_slice(&[o[0], o[1], o[2], 0.0]);
        }
        match &self.texture {
            Some(texture) => texture.put_back_vec_f32(cx, data, None),
            None => {
                self.texture = Some(Texture::new_with_format(
                    cx,
                    TextureFormat::VecRGBAf32 {
                        width: params.cols,
                        height: params.rows,
                        data: Some(data),
                        updated: TextureUpdated::Full,
                    },
                ));
            }
        }
        self.texture.clone()
    }
}

/// Rest depth at picture coordinates `u, v`, matching the shader's mapping.
fn rest_z(rest: &RestDepth, u: f32, v: f32) -> Option<f32> {
    match *rest {
        RestDepth::Plane(z) => Some(z),
        RestDepth::Map {
            width,
            height,
            values,
            inv_far,
        } => {
            if width < 2 || height < 2 || values.len() != width * height {
                return None;
            }
            let x = (u * width as f32 - 0.5).clamp(0.0, (width - 1) as f32);
            let y = (v * height as f32 - 0.5).clamp(0.0, (height - 1) as f32);
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
            let (fx, fy) = (x - x0 as f32, y - y0 as f32);
            let at = |xx: usize, yy: usize| values[yy * width + xx];
            let taps = [at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)];
            if taps.iter().any(|n| *n < 0.0) {
                return None;
            }
            let n = (taps[0] * (1.0 - fx) + taps[1] * fx) * (1.0 - fy)
                + (taps[2] * (1.0 - fx) + taps[3] * fx) * fy;
            Some(1.0 / (inv_far + (1.0 - inv_far) * n))
        }
    }
}
