//! B3, the animator: the kit's `glyph: fn(g, o)` compiled as a kernel over
//! the elements. `g` is the element's `Glyph` record (records.rs), `o` the
//! draw record (`DrawKineticGlyph`'s instance fields, reflected), set to
//! rest before the body runs; the body changes what it wants and must not
//! `return` (the record is written after it).
//!
//! Signals are params (set every frame, no recompile): `beat phase pulse
//! bar bpm energy grow`, the text's `width height size`, the shape ids
//! `alpha0 alphas cube`, `text_at` (the time the text last changed),
//! `bass mid high` (0..1 bands), `floor_y` (the floor's height),
//! `view_w view_h` (a picture's world extent, 0 without one), `band(f)`
//! (the spectrum at log frequency f 0..1, the host's 32 bands), the
//! dials `p1..p10` and each dial by its own name (a dial the kit holds
//! past the tenth too, at its default: no host sets it);
//! `time` is the kernel's own time input. The `kinetic` module
//! (kinetic.splash) is imported unqualified.

use crate::kit::Kit;
use crate::records::glyph_layout;
use makepad_draw::*;
use makepad_script_compute::kernel::{Kernel, Layout, MathMode};
use makepad_script_compute::module::Module;
use makepad_script_compute::vm_kernel::{self, Decl, Entry, VmKernel};
use makepad_script_compute::Backend;
use std::sync::Arc;

/// The stock motions module.
pub const KINETIC_MODULE: &str = include_str!("kinetic.splash");

/// The signals every animator has, in this order.
pub const SIGNALS: &[(&str, f32)] = &[
    ("beat", 0.0),
    ("phase", 0.0),
    ("pulse", 0.0),
    ("bar", 0.0),
    ("bpm", 120.0),
    ("energy", 0.0),
    ("grow", 1.0),
    ("width", 1.0),
    ("height", 1.0),
    ("size", 1.0),
    ("alpha0", 0.0),
    ("alphas", 0.0),
    ("cube", 0.0),
    ("text_at", -1000000000.0),
    ("bass", 0.0),
    ("mid", 0.0),
    ("high", 0.0),
    ("floor_y", 0.0),
    ("view_w", 0.0),
    ("view_h", 0.0),
];

/// Which of a kit's kernels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `glyph: fn(g, o)` over the elements (a kit without one: at rest).
    Glyph,
    /// `camera_fn: fn(c)`, one element from the default framing.
    Camera,
    /// `curve_fn: fn(c)`, a point per element.
    Curve,
}

/// Compile one of a kit's kernels (see [`Which`]) with the glyph draw's
/// record layout `out`; errors name the kit's file, line and column.
pub fn compile(vm: &ScriptVm, kit: &Kit, which: Which, out: &Layout) -> Result<Arc<Kernel>, String> {
    let glue = |name: &str| -> Result<ScriptObject, String> {
        let m = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str(crate::kit::KIT_MODULE).into(), NoTrap).as_object().ok_or("the kit module is not registered")?;
        vm.bx.heap.value(m, LiveId::from_str(name).into(), NoTrap).as_object().ok_or_else(|| format!("kit.splash has no `{name}`"))
    };
    let io = |name: &str, ty: &str, input: bool| {
        let (name, ty) = (name.to_string(), ty.to_string());
        if input {
            Decl::Input { name, ty, stride: None, offset: None, buffer: None }
        } else {
            Decl::Output { name, ty, stride: None, offset: None, buffer: None }
        }
    };
    let (entry, entry_fn, bind, mut decls) = match which {
        Which::Glyph => {
            let f = match kit.fn_field(vm, "glyph") {
                Some(f) => f,
                None => glue("still")?,
            };
            (Entry::Instance, glue("glyph_entry")?, ("kit_glyph", f), vec![io("glyphs", "Glyph", true), io("out", &out.name, false)])
        }
        Which::Camera => {
            let f = kit.fn_field(vm, "camera_fn").ok_or("the kit has no camera_fn")?;
            (Entry::Element, glue("camera_entry")?, ("kit_camera", f), vec![io("base", "Camera", true), io("out", "Camera", false)])
        }
        Which::Curve => {
            let f = kit.fn_field(vm, "curve_fn").ok_or("the kit has no curve_fn")?;
            (Entry::Element, glue("curve_entry")?, ("kit_curve", f), vec![io("out", "CurvePoint", false)])
        }
    };
    // The newest spectrum in 32 log bands (`band(f)`).
    decls.push(io("spectrum", "f32", true));
    let param = |name: &str, default: f32, range: f32| Decl::Param { name: name.to_string(), default, range: Some((-range, range)) };
    for (name, default) in SIGNALS {
        decls.push(param(name, *default, 1000000000.0));
    }
    let dials = &kit.values.dials;
    for k in 0..crate::kit::MAX_DIALS {
        decls.push(param(&format!("p{}", k + 1), dials.get(k).map_or(0.5, |d| d.1), 1000.0));
    }
    for (name, d) in dials.iter().chain(&kit.values.held) {
        decls.push(param(name, *d, 1000.0));
    }
    let k = VmKernel { decls, entry, entry_fn, math: MathMode::Fast, uses: vec!["kinetic".into()], bind: vec![(bind.0.to_string(), bind.1)] };
    let layouts = [glyph_layout(), out.clone(), camera_layout(), curve_layout()];
    let modules = [Module { path: "kinetic", source: KINETIC_MODULE }];
    vm_kernel::compile(vm, &k, &layouts, Backend::Native, &modules).map(|(kernel, _)| kernel).map_err(|errors| errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"))
}

/// The camera record a `camera_fn: fn(c)` writes. `share` is the kit's
/// own: four values the camera fn works out once a frame (from the dials,
/// the beat, the sound) that every look and backdrop reads as
/// `self.k_share`, so the shaders do not repeat the maths.
pub fn camera_layout() -> Layout {
    use makepad_script_compute::kernel::{FieldTy, LayoutField};
    let f = |name: &str, ty, offset| LayoutField { name: name.into(), ty, offset };
    Layout {
        name: "Camera".into(),
        stride: 16,
        fields: vec![f("eye", FieldTy::Vec3, 0), f("target", FieldTy::Vec3, 3), f("up", FieldTy::Vec3, 6), f("fov", FieldTy::F32, 9), f("roll", FieldTy::F32, 10), f("share", FieldTy::Vec4, 12)],
    }
}

/// The record a `curve_fn: fn(c)` writes: its point at parameter `u`.
pub fn curve_layout() -> Layout {
    use makepad_script_compute::kernel::{FieldTy, LayoutField};
    Layout { name: "CurvePoint".into(), stride: 4, fields: vec![LayoutField { name: "pos".into(), ty: FieldTy::Vec3, offset: 0 }, LayoutField { name: "u".into(), ty: FieldTy::F32, offset: 3 }] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_script_compute::kernel::{FieldTy, LayoutField};

    fn out_layout() -> Layout {
        let f = |name: &str, ty, offset| LayoutField { name: name.into(), ty, offset };
        Layout {
            name: "KineticGlyph".into(),
            stride: 24,
            fields: vec![
                f("pos", FieldTy::Vec3, 0),
                f("rot", FieldTy::Vec4, 3),
                f("scale", FieldTy::Vec3, 7),
                f("shear", FieldTy::Vec2, 10),
                f("color", FieldTy::Vec4, 12),
                f("attr", FieldTy::Vec4, 16),
                f("info", FieldTy::Vec4, 20),
                f("shape", FieldTy::F32, 23),
            ],
        }
    }

    fn with_kit<R>(src: &str, f: impl FnOnce(&mut ScriptVm, Result<Kit, String>) -> R) -> R {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_draw::script_mod(vm);
            crate::view::script_mod(vm);
            let kit = crate::kit::load(vm, src, "test_kit");
            f(vm, kit)
        })
    }

    #[test]
    fn a_kit_animator_compiles_runs_and_names_its_lines() {
        let src = "fn lift(g) { g.t * 2.0 }\nKinetic{\n  dials: {swing: 0.25}\n  glyph: fn(g, o) {\n    let w = wave(g.rest.x, width, size, time, 1.0 + swing, 1.0, pulse)\n    o.pos.y = o.pos.y + w.x + lift(g)\n    o.rot = qz(w.y)\n  }\n}\n";
        let k = with_kit(src, |vm, kit| compile(vm, &kit.unwrap(), Which::Glyph, &out_layout())).unwrap_or_else(|e| panic!("{e}"));
        let mut recs = vec![0.0f32; 2 * 40];
        recs[14] = 0.0;
        recs[40 + 14] = 1.0;
        recs[40] = 1.0;
        recs[35] = 1.0;
        recs[40 + 35] = 1.0;
        let mut out = vec![0.0f32; 2 * 24];
        let spec = [0.5f32; 32];
        let mut call = k.call();
        call.set_time(0.3);
        call.input("spectrum", &spec).unwrap();
        call.set_param("width", 2.0);
        call.input("glyphs", &recs).unwrap();
        call.output("out", &mut out).unwrap();
        call.run(2).unwrap();
        drop(call);
        assert!(out[24 + 1] > 1.5, "the second glyph lifted by lift(g) = 2: {}", out[25]);
        assert!((out[3..7].iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-4, "a unit quaternion");
        // A camera kernel writes its record from the default framing.
        let cam = "Kinetic{\n  camera_fn: fn(c) {\n    c.eye = vec3(sin(time) * 5.0, 1.0, cos(time) * 5.0)\n    c.fov = c.fov + 10.0\n  }\n}\n";
        let ck = with_kit(cam, |vm, kit| compile(vm, &kit.unwrap(), Which::Camera, &out_layout())).unwrap_or_else(|e| panic!("{e}"));
        let base = [0.0f32, 0.0, 9.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 50.0, 0.0, 0.0];
        let mut o = [0.0f32; 12];
        let mut call = ck.call();
        call.input("base", &base).unwrap();
        call.input("spectrum", &spec).unwrap();
        call.output("out", &mut o).unwrap();
        call.run(1).unwrap();
        drop(call);
        assert_eq!((o[1], o[7], o[9]), (1.0, 1.0, 60.0), "{o:?}");
        let bad = "Kinetic{\n  glyph: fn(g, o) {\n    o.pos.q = 1.0\n  }\n}\n";
        let e = with_kit(bad, |vm, kit| compile(vm, &kit.unwrap(), Which::Glyph, &out_layout())).err().expect("an error");
        assert!(e.contains("test_kit:3:"), "{e}");
        // The dials, in the order written.
        let dials = with_kit("Kinetic{ dials: {zeta: 0.1 alpha_amt: 0.9 mid_amt: 0.3} }", |_, kit| kit.unwrap().values.dials);
        assert_eq!(dials, vec![("zeta".to_string(), 0.1), ("alpha_amt".to_string(), 0.9), ("mid_amt".to_string(), 0.3)]);
        assert!(with_kit("Nope{}", |_, kit| kit.is_err()));
        // Each of the ten dial slots has its shader accessor.
        let fns = with_kit("Kinetic{ dials: {a: 0.1 b: 0.2 c: 0.3 d: 0.4} }", |vm, kit| {
            let m = vm.bx.heap.value(vm.bx.heap.modules, LiveId::from_str(crate::kit::KIT_MODULE).into(), NoTrap).as_object().unwrap();
            drop(kit);
            ["dial_x", "dial_y", "dial_z", "dial_w", "dial_b_x", "dial_b_y", "dial_b_z", "dial_b_w", "dial_c_x", "dial_c_y"].iter().filter(|k| vm.bx.heap.value(m, LiveId::from_str(k).into(), NoTrap).as_object().is_some_and(|f| vm.bx.heap.as_fn(f).is_some())).count()
        });
        assert_eq!(fns, 10);
    }

    /// A kit with six dials: the host's values reach the glyph fn, by slot
    /// (`p4..p6`) and by name, the kit's defaults stand where the host sets
    /// none, a slot past the kit's list is 0.5, and every dial has its
    /// shader accessor `self.<dial>()`.
    #[test]
    fn a_kit_with_six_dials_reaches_the_glyph_function() {
        let src = "Kinetic{\n  dials: {alpha_amt: 0.1 beta_amt: 0.2 gamma_amt: 0.3 delta_amt: 0.4 eps_amt: 0.5 zeta_amt: 0.6}\n  glyph: fn(g, o) {\n    o.pos = vec3(p5, p6, zeta_amt)\n    o.scale = vec3(eps_amt, p4, delta_amt)\n  }\n}\n";
        let (out, members) = with_kit(src, |vm, kit| {
            let kit = kit.unwrap_or_else(|e| panic!("{e}"));
            let mut frame = crate::view::KineticFrame::default();
            frame.dials[3] = Some(0.75);
            frame.dials[4] = Some(0.85);
            frame.dials[5] = Some(0.9);
            let p = crate::view::dial_values(&kit.values, &[], &frame);
            assert_eq!(p, [0.1, 0.2, 0.3, 0.75, 0.85, 0.9, 0.5, 0.5, 0.5, 0.5], "the kit's defaults, the host's fourth to sixth, 0.5 past the list");
            let k = compile(vm, &kit, Which::Glyph, &out_layout()).unwrap_or_else(|e| panic!("{e}"));
            let recs = vec![0.0f32; 40];
            let spec = [0.0f32; 32];
            let mut out = vec![0.0f32; 24];
            let mut call = k.call();
            call.input("spectrum", &spec).unwrap();
            call.input("glyphs", &recs).unwrap();
            call.output("out", &mut out).unwrap();
            crate::view::set_dial_params(&mut call, &p, &kit.values.dials);
            call.run(1).unwrap();
            drop(call);
            let members: Vec<String> = crate::view::members(vm, &kit, false).iter().map(|(id, _)| id.to_string()).collect();
            (out, members)
        });
        assert_eq!(out[0..3], [0.85, 0.9, 0.9], "p5, p6 and the sixth dial by its name");
        assert_eq!(out[7..10], [0.85, 0.75, 0.75], "the fifth by its name, p4, the fourth by its name");
        for name in ["alpha_amt", "beta_amt", "gamma_amt", "delta_amt", "eps_amt", "zeta_amt"] {
            assert!(members.iter().any(|m| m == name), "`self.{name}()` in the shaders: {members:?}");
        }
        // Ten are the dials a kit has: an eleventh is held, not refused.
        let eleven = (0..11).map(|k| format!("d{k}_amt: 0.5")).collect::<Vec<_>>().join(" ");
        let held = with_kit(&format!("Kinetic{{ dials: {{{eleven}}} }}"), |_, kit| kit.map(|k| (k.values.dials.len(), k.values.held)));
        assert_eq!(held, Ok((crate::kit::MAX_DIALS, vec![("d10_amt".to_string(), 0.5)])));
        let ten = (0..10).map(|k| format!("d{k}_amt: 0.5")).collect::<Vec<_>>().join(" ");
        assert_eq!(with_kit(&format!("Kinetic{{ dials: {{{ten}}} }}"), |_, kit| kit.map(|k| (k.values.dials.len(), k.values.held.len()))), Ok((10, 0)));
    }
}
