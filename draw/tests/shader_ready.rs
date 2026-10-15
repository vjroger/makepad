//! The shader readiness answer on the native HAL GPU backend (D3D11): a draw shader is not ready
//! to draw, and draw shaders count as pending, from the moment it is queued until the HAL said
//! its pipeline is built; after a device reset too, when the HAL builds every pipeline again on
//! the new device. Run with the ignored test selection.
#![cfg(windows)]

use makepad_draw::makepad_platform::makepad_hal::{self as hal_api, Hal, OpenReq, open_flags};
use makepad_draw::*;
use std::{cell::RefCell, rc::Rc, time::{Duration, Instant}};

const SIZE: f64 = 64.0;

#[derive(Default)]
struct Seen {
    shader: Option<DrawShaderId>,
    /// (ready, pending) when the shader was made (queued, no pipeline yet)
    at_startup: Option<(bool, bool)>,
    /// (ready, pending) when the app heard the GPU device was replaced
    at_reset: Option<(bool, bool)>,
}

/// Turns until `heard` holds, the shader is ready, nothing is pending and a frame drawn since is
/// complete. On every turn a shader that cannot draw yet must count as pending.
fn until_ready(cx: &Rc<RefCell<Cx>>, seen: &Rc<RefCell<Seen>>, what: &str, heard: impl Fn(&Seen) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut ready_at = None;
    loop {
        assert!(Instant::now() < deadline, "{what}: the shader never became ready");
        let mut cx = cx.borrow_mut();
        assert!(cx.hal_session_turn(1_000_000, true));
        let Some(id) = seen.borrow().shader else { continue };
        let (ready, pending) = (cx.draw_shader_ready(id, false), cx.draw_shaders_pending());
        assert!(ready || pending, "{what}: a shader that cannot draw yet counts as pending");
        if ready && !pending && heard(&seen.borrow()) {
            // a frame drawn after that, and complete
            let serial = *ready_at.get_or_insert(cx.frame_submission_serial());
            if cx.frame_submission_serial() > serial {
                if cx.frame_completion_serial() == cx.frame_submission_serial() {
                    return;
                }
                continue;
            }
        }
        cx.redraw_all();
    }
}

#[test]
#[ignore = "requires a native HAL GPU"]
fn a_shader_is_not_ready_until_its_pipeline_is_built_and_again_after_a_device_reset() {
    let seen = Rc::new(RefCell::new(Seen::default()));
    let app_seen = seen.clone();
    let mut state = None;
    let cx = Rc::new(RefCell::new(Cx::new(Box::new(move |cx, event| match event {
        Event::Startup => {
            let quad = cx.with_vm(|vm| {
                makepad_platform::script::script_mod(vm);
                makepad_draw::script_mod(vm);
                let mut quad = DrawColor::script_new_with_default(vm);
                quad.color = vec4(1.0, 0.5, 0.0, 1.0);
                quad
            });
            let id = quad.draw_vars.draw_shader_id.expect("DrawColor has a shader");
            app_seen.borrow_mut().shader = Some(id);
            app_seen.borrow_mut().at_startup = Some((cx.draw_shader_ready(id, false), cx.draw_shaders_pending()));
            let mut window = WindowHandle::new(cx);
            window.configure_window(cx, dvec2(SIZE, SIZE), dvec2(0.0, 0.0), false, "Shader readiness".into());
            let pass = DrawPass::new(cx);
            window.set_pass(cx, &pass);
            pass.set_window_clear_color(cx, vec4(0.0, 0.0, 0.0, 1.0));
            state = Some((window, pass, DrawList2d::new(cx), quad));
            cx.redraw_all();
        }
        Event::Draw(event) => {
            let (_, pass, root, quad) = state.as_mut().unwrap();
            let mut draw = CxDraw::new(cx, event);
            let mut cx = Cx2d::new(&mut draw);
            cx.begin_pass(pass, None);
            root.begin_always(&mut cx);
            cx.begin_root_turtle(dvec2(SIZE, SIZE), Layout::flow_overlay());
            quad.draw_abs(&mut cx, Rect { pos: dvec2(8.0, 8.0), size: dvec2(SIZE - 16.0, SIZE - 16.0) });
            cx.end_pass_sized_turtle();
            root.end(&mut cx);
            cx.end_pass(pass);
        }
        Event::GpuReset => {
            let id = app_seen.borrow().shader.unwrap();
            app_seen.borrow_mut().at_reset = Some((cx.draw_shader_ready(id, false), cx.draw_shaders_pending()));
        }
        _ => {}
    }))));
    cx.borrow_mut().init_cx_os();
    let hal = Hal::global();
    let mut req = OpenReq::from_env(0, open_flags::OFFSCREEN_WINDOWS, 1.0);
    req.flags |= open_flags::OFFSCREEN_WINDOWS | open_flags::RETAIN_WINDOWS;
    let info = hal.open(&req);
    assert_eq!(info.backend, hal_api::backend_id::D3D11, "the D3D11 backend");
    cx.borrow_mut().hal_attach_session(info);
    let skipped = || hal.with_world(|w| w.session(info.session).map_or(0, |s| s.stats.skipped_draws));

    let orange = |what: &str| {
        let pixels = hal.window_pixels(info.session, None).unwrap_or_else(|| panic!("{what}: the window's pixels"));
        // the quad's centre (the window is at least its minimum size)
        let centre = (SIZE as usize / 2 * pixels.w as usize + SIZE as usize / 2) * 4;
        let [r, g, b] = [pixels.rgba[centre], pixels.rgba[centre + 1], pixels.rgba[centre + 2]];
        assert!(r > 240 && (100..160).contains(&g) && b < 20, "{what}: the quad is drawn (centre {r} {g} {b})");
    };

    // first use: queued, then compiling on the HAL's workers, then ready
    until_ready(&cx, &seen, "first use", |_| true);
    assert_eq!(seen.borrow().at_startup, Some((false, true)), "a queued shader is not ready and is pending");
    // the app held its frames until the pipeline was built: the HAL left no draw out
    assert_eq!(skipped(), 0, "no draw was left out of a frame");
    orange("first use");

    // a device reset: every pipeline is built again on the new device
    hal.simulate_device_loss();
    until_ready(&cx, &seen, "after the device reset", |s| s.at_reset.is_some());
    assert_eq!(seen.borrow().at_reset, Some((false, true)), "after a device reset the shader is not ready until its pipeline is built again");
    assert_eq!(skipped(), 0, "no draw was left out of a frame after the device reset");
    orange("after the device reset");
    hal.close(info.session);
}
