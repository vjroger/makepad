//! HDRI phase 2 (C8): the environment preparation (prefilter atlas, dome,
//! SH9, meter) runs exactly once per key — never per frame, never per poll
//! — a re-registered map is a new key, and a key that changes while its job
//! runs ends with the last key's texture. Device-free: `Cx::new` gives a
//! pool (or a closed one, where C3 prepares synchronously; the counts are
//! the same either way).
use super::*;
use makepad_render_material::ibl::{sh9_irradiance, EnvMap};
use makepad_scene::{Environment, Ibl, IblSource, TextureRef};
use std::sync::Arc;

fn env_for(source: IblSource) -> Environment {
    Environment { ibl: Some(Ibl { source, intensity: 1.0, rotation_deg: 0.0 }), ..Default::default() }
}

/// A world that carries `env` (`env_lighting` takes the world).
fn world_of(env: &Environment) -> World {
    let mut w = World::new();
    w.environment = *env;
    w
}

/// Drive `resolve_ibl` until the preparation for `env` has landed. A
/// preparation is a real `prefilter(.., 256, 6)` on a debug build (the
/// workspace's dev profile sets no opt-level) while the rest of
/// `renderer::` runs in parallel: seconds each, so the guard is a clock,
/// as in renderer/ibl.rs's own tests, not a poll count.
#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
fn settle(r: &mut Renderer, cx: &mut Cx, env: &Environment) {
    let start = std::time::Instant::now();
    loop {
        r.resolve_ibl(cx, env);
        if r.ibl_texture().is_some() && !r.ibl_pending() {
            return;
        }
        assert!(start.elapsed().as_secs() < 180, "the environment never prepared");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn a_key_change_prepares_exactly_once() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut r = Renderer::default();
    r.register_environment(TextureRef(7), Arc::new(EnvMap::constant(32, [0.5, 0.6, 0.7])));
    r.register_environment(TextureRef(8), Arc::new(EnvMap::constant(32, [0.9, 0.2, 0.1])));
    let seven = env_for(IblSource::Hdri(TextureRef(7)));
    let eight = env_for(IblSource::Hdri(TextureRef(8)));

    settle(&mut r, &mut cx, &seven);
    assert_eq!(r.ibl_preparations(), 1, "the first frame prepares");
    let first = r.ibl_texture().unwrap().texture_id();
    for _ in 0..60 {
        r.resolve_ibl(&mut cx, &seven);
    }
    assert_eq!(r.ibl_preparations(), 1, "an unchanged key never prepares again");
    assert_eq!(r.ibl_texture().unwrap().texture_id(), first, "and keeps its texture");
    assert!(r.env_lighting(&world_of(&seven)).is_some(), "the rig sees the preparation");

    settle(&mut r, &mut cx, &eight);
    assert_eq!(r.ibl_preparations(), 2, "a new source prepares once");
    assert_ne!(r.ibl_texture().unwrap().texture_id(), first);
    let sh = *r.ibl_sh9().expect("the landed SH");
    let e = sh9_irradiance(&sh, [0.0, 1.0, 0.0]);
    assert!(e[0] > e[2], "eight is red over blue: {e:?}");

    // A key that changes while its job may still be running: exactly one
    // preparation for the abandoned key (C3 cancels it as stale the moment
    // the world names eight again, and eight is still bound, so nothing
    // more is submitted), and the texture is the LAST key's. 4 would mean
    // the stale job landed and eight had to be prepared again.
    r.resolve_ibl(&mut cx, &seven);
    settle(&mut r, &mut cx, &eight);
    let after_flip = r.ibl_preparations();
    assert!((3..=4).contains(&after_flip), "{after_flip}");
    let sh = *r.ibl_sh9().expect("the landed SH");
    let e = sh9_irradiance(&sh, [0.0, 1.0, 0.0]);
    assert!(e[0] > e[2], "the stale result was dropped: {e:?}");
    for _ in 0..60 {
        r.resolve_ibl(&mut cx, &eight);
    }
    assert_eq!(r.ibl_preparations(), after_flip, "settled again");

    // Re-registering the CURRENT source is a change (renderer/ibl.rs drops
    // its key), so the new map prepares exactly once; until it lands the
    // rig keeps the old map's numbers (no flicker on a day-cycle re-bake).
    r.register_environment(TextureRef(8), Arc::new(EnvMap::constant(32, [0.1, 0.1, 0.9])));
    r.resolve_ibl(&mut cx, &eight);
    if r.ibl_pending() {
        let e = sh9_irradiance(r.ibl_sh9().expect("the old numbers stay while the new map prepares"), [0.0, 1.0, 0.0]);
        assert!(e[0] > e[2], "still the old (red) map's SH: {e:?}");
        assert!(r.env_lighting(&world_of(&eight)).is_some(), "the rig does not fall back to the analytic values");
    }
    settle(&mut r, &mut cx, &eight);
    assert_eq!(r.ibl_preparations(), after_flip + 1);
    let e = sh9_irradiance(r.ibl_sh9().expect("the landed SH"), [0.0, 1.0, 0.0]);
    assert!(e[2] > e[0], "the re-registered map landed: {e:?}");
    // Re-registering a source that is NOT current prepares nothing now.
    r.register_environment(TextureRef(7), Arc::new(EnvMap::constant(32, [0.3, 0.3, 0.3])));
    for _ in 0..30 {
        r.resolve_ibl(&mut cx, &eight);
    }
    assert_eq!(r.ibl_preparations(), after_flip + 1);

    // No environment: nothing prepares and the texture is gone; naming an
    // unregistered handle is the same.
    r.resolve_ibl(&mut cx, &Environment::default());
    assert!(r.ibl_texture().is_none());
    assert!(r.env_lighting(&world_of(&eight)).is_none(), "no preparation, no rig contribution");
    r.resolve_ibl(&mut cx, &env_for(IblSource::Hdri(TextureRef(99))));
    assert!(r.ibl_texture().is_none() && !r.ibl_pending());
    assert_eq!(r.ibl_preparations(), after_flip + 1);
}
