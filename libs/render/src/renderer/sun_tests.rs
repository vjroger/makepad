use super::*;

/// T7 on the game side: every game shader must read ONE sun. Before the
/// unification each shader carried its own ambient/direct constants and
/// five script blocks set the light direction by hand, so changing one
/// silently left the others behind. `write_into` is the single write
/// path — apply_sun calls it once per shader and draw_skinned_inner
/// calls it for the skinned struct, so uniformity is compiler-enforced
/// and this asserts the payload rather than eyeballing a capture.
#[test]
fn write_into_sets_every_sun_field() {
    let sun = SunLight::from_time_of_day(8.0, 52.0);
    let (mut dir, mut color, mut sky, mut ground) = (
        Vec3f::default(),
        Vec3f::default(),
        Vec3f::default(),
        Vec3f::default(),
    );
    sun.write_into(&mut dir, &mut color, &mut sky, &mut ground);
    assert_eq!(dir, sun.dir);
    assert_eq!(color, sun.color);
    assert_eq!(sky, sun.sky);
    assert_eq!(ground, sun.ground);
}

/// Two shaders fed by the same sun end up with identical values — the
/// property that used to fail silently.
#[test]
fn two_targets_receive_identical_values() {
    let sun = SunLight::from_time_of_day(17.0, 52.0);
    let mut a = [Vec3f::default(); 4];
    let mut b = [Vec3f::default(); 4];
    let [a0, a1, a2, a3] = &mut a;
    sun.write_into(a0, a1, a2, a3);
    let [b0, b1, b2, b3] = &mut b;
    sun.write_into(b0, b1, b2, b3);
    assert_eq!(a, b);
}

/// A default world must light exactly as the pre-unification shaders
/// did, so adopting SceneSun did not restyle every existing game.
#[test]
fn the_default_sun_is_the_legacy_look() {
    let sun = crate::sun::resolve_sun(&makepad_scene::SunConfig::default());
    assert_eq!(sun, SunLight::default());
    // Flat hemisphere collapses mix(ground, sky, h) to the old constant.
    assert_eq!(sun.sky, sun.ground);
}

/// A painted sky on a running clock takes the analytic dome — so it sets,
/// dusks and goes dark — tinted by its palette in daylight and neutral at
/// night; a painted sky under a fixed hour keeps its gradient.
#[test]
fn a_painted_sky_on_a_clock_runs_the_day_cycle() {
    let mut world = World::new();
    world.sky = Some(makepad_scene::SkyConfig {
        top: vec4(0.62, 0.71, 0.84, 1.0),
        ..Default::default()
    });
    world.sun.time_of_day = Some(12.0);
    world.sun.latitude = 52.0;
    let noon = crate::sun::solar_dir(12.0, 52.0);
    assert!(analytic_sky_frame(&world, noon, true, true, false).is_none(), "a fixed hour keeps the painted gradient");
    let day = analytic_sky_frame(&world, noon, true, true, true).expect("a running clock takes the analytic dome");
    assert!(day.dome_tint.x > day.dome_tint.z, "a pale, warm top tints the day dome");
    let night = analytic_sky_frame(&world, crate::sun::solar_dir(0.0, 52.0), true, true, true).unwrap();
    assert!((night.dome_tint - vec3f(1.0, 1.0, 1.0)).length() < 1.0e-4, "every night is the stock night");
    world.sky = Some(makepad_scene::SkyConfig::default());
    let stock = analytic_sky_frame(&world, noon, true, true, false).unwrap();
    assert_eq!(stock.dome_tint, vec3f(1.0, 1.0, 1.0), "the stock sky is untinted");
}

fn env_sun_of(dir: Vec3f, radiance: f32, radius_deg: f32, facing: f32) -> makepad_scene::EnvSun {
    let cos_radius = radius_deg.to_radians().cos();
    makepad_scene::EnvSun {
        dir: dir.normalize(),
        radiance: vec3f(radiance, radiance, radiance),
        cos_radius,
        facing,
        cos_cover: cos_radius,
    }
}

fn env_named(source: makepad_scene::IblSource, sun: Option<makepad_scene::EnvSun>) -> World {
    let mut world = World::new();
    world.environment.ibl = Some(makepad_scene::Ibl { source, intensity: 1.0, rotation_deg: 0.0 });
    world.environment.sun = sun;
    world
}

/// Phase 2: with no environment prepared, the environment's share of the
/// rig is a no-op — the legacy look stays bit for bit, also for a world
/// that names an IBL the renderer has not prepared.
#[test]
fn without_a_prepared_environment_the_rig_is_untouched() {
    let renderer = Renderer::default();
    let world = World::new();
    let sun = crate::sun::resolve_sun(&world.sun);
    assert_eq!(renderer.env_sun_rig(&world, sun), sun);
    assert_eq!(renderer.env_sun_rig(&world, sun), SunLight::default());
    assert!(renderer.env_sun_dir(&world).is_none());
    let named = env_named(
        makepad_scene::IblSource::Procedural(0),
        Some(makepad_scene::EnvSun {
            dir: vec3f(0.0, 1.0, 0.0),
            radiance: vec3f(1.0, 1.0, 1.0),
            cos_radius: 0.99,
            facing: 1.0,
            cos_cover: 0.98,
        }),
    );
    assert_eq!(renderer.env_sun_rig(&named, sun), sun, "named but unprepared: nothing changes");
    assert!(renderer.env_sun_dir(&named).is_none());
    assert!(!renderer.ibl_pending());
    assert_eq!(renderer.ibl_preparations(), 0);
}

/// The meter counts the key the way the directional light delivers it: its
/// share of the sphere's mean, `L (1 - cos r) / 2`, times what a surface
/// facing it receives (`facing`), on top of the lighting copy's mean (which
/// has the key's cone filled). Only a world that names an IBL reads the
/// renderer's numbers: an aux draw's default environment never picks up the
/// scene's.
#[test]
fn the_meter_adds_the_keys_delivered_share_back_and_only_for_a_world_that_names_an_ibl() {
    let mut renderer = Renderer::default();
    let sh = makepad_render_material::ibl::sh9(&makepad_render_material::ibl::EnvMap::constant(32, [0.5, 0.5, 0.5]));
    renderer.feed_environment_numbers_for_tests(sh, 0.5, vec3f(0.5, 0.5, 0.5));
    let world = env_named(makepad_scene::IblSource::Hdri(makepad_scene::TextureRef(1)), None);
    let sunless = renderer.env_lighting(&world).expect("a landed preparation under a world that names an IBL");
    assert_eq!(sunless.mean_luminance, 0.5, "no key, no share");
    assert_eq!(sunless.gain, 1.0);
    let wide = env_sun_of(vec3f(0.3, 0.8, -0.5), 10.0, 55.0, 0.78);
    let whole = makepad_scene::EnvSun { facing: 1.0, ..wide };
    let share = |sun: makepad_scene::EnvSun, intensity: f32| {
        let mut world = env_named(makepad_scene::IblSource::Hdri(makepad_scene::TextureRef(1)), Some(sun));
        world.environment.ibl.as_mut().unwrap().intensity = intensity;
        renderer.env_lighting(&world).expect("lighting").mean_luminance - 0.5
    };
    let want = 10.0 * (1.0 - 55.0_f32.to_radians().cos()) * 0.5;
    assert!((share(whole, 1.0) - want).abs() < 1.0e-4, "{} vs {want}", share(whole, 1.0));
    assert!((share(wide, 1.0) - want * 0.78).abs() < 1.0e-4, "the share carries the key's facing: {}", share(wide, 1.0));
    // The intensity is the gain, not part of the mean (the meter multiplies it).
    assert!((share(wide, 2.0) - want * 0.78).abs() < 1.0e-4);
    assert_eq!(renderer.env_lighting(&world).unwrap().gain, 1.0);
    // An invalid key is not the world's: the renderer reads none from it.
    let broken = makepad_scene::EnvSun { facing: 2.0, ..wide };
    assert_eq!(share(broken, 1.0), 0.0);
    // A world that names none gets nothing, whatever the scene last bound.
    let aux = World::new();
    assert!(renderer.env_lighting(&aux).is_none());
    let sun = crate::sun::resolve_sun(&aux.sun);
    assert_eq!(renderer.env_sun_rig(&aux, sun), sun);
    assert!(renderer.env_sun_dir(&aux).is_none());
}

/// The whole path on a real preparation: a registered map with a hot disc
/// and the sun declared for it. The frame's rig points at the sun, carries
/// its colour and the map's fill, meters the map (disc included) in both
/// lanes, and a world that no longer declares the sun keeps the bound one
/// until the new preparation lands (the dome and the light are one map).
#[test]
fn a_landed_environment_lights_the_rig_in_both_lanes() {
    let mut cx = Cx::new(Box::new(|_, _| {}));
    let mut renderer = Renderer::default();
    let toward = crate::hdri::dir_from_az_el(90.0, 45.0);
    let sun = env_sun_of(toward, 1.0e3, 3.0, 1.0);
    let map = makepad_render_material::ibl::EnvMap::from_fn(128, |d| {
        if crate::hdri::vec(d).dot(toward) >= sun.cos_radius { [1.0e3; 3] } else { [0.5; 3] }
    });
    renderer.register_environment(makepad_scene::TextureRef(1), std::sync::Arc::new(map));
    let mut world = env_named(makepad_scene::IblSource::Hdri(makepad_scene::TextureRef(1)), Some(sun));
    // The clock and the sleep are the test's: it runs natively with a real pool.
    #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
    let settle = |renderer: &mut Renderer, cx: &mut Cx, world: &World| {
        let start = std::time::Instant::now();
        loop {
            renderer.resolve_ibl(cx, &world.environment);
            if !renderer.ibl_pending() {
                return;
            }
            assert!(start.elapsed().as_secs() < 180, "the preparation did not finish within 180 s");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    };
    settle(&mut renderer, &mut cx, &world);
    assert_eq!(renderer.ibl_preparations(), 1);
    let stock = SunLight::default();

    // The lighting copy's mean is the 0.5 sky (the cone is filled); the key's
    // share, L (1 - cos r) / 2 = 0.69, is added back for the meter.
    let key_share = 1.0e3 * (1.0 - sun.cos_radius) * 0.5;
    let lighting = renderer.env_lighting(&world).expect("landed");
    assert!((lighting.mean_luminance - (0.5 + key_share)).abs() < 0.02, "{} vs {}", lighting.mean_luminance, 0.5 + key_share);
    assert_eq!(renderer.env_sun(&world), Some(sun));
    let dir = renderer.env_sun_dir(&world).expect("the environment places the sun");
    assert!((dir - toward).length() < 1.0e-4, "{dir:?}");

    // HDR lane: the map's own units, the composite exposes.
    renderer.hdr_output = true;
    let hdr = renderer.env_sun_rig(&world, stock.to_hdr());
    assert!(hdr.dir.dot(toward) > 0.9999, "{:?}", hdr.dir);
    let want = 1.0e3 * 2.0 * (1.0 - sun.cos_radius);
    assert!((hdr.color.x - want).abs() < 2.0e-3 * want, "direct {:?} vs {want}", hdr.color);
    assert!((hdr.sky.x - 0.5).abs() < 0.01 && (hdr.ground.x - 0.5).abs() < 0.01, "the fill is the sky's: {:?} {:?}", hdr.sky, hdr.ground);
    let exposure = crate::sun::env_exposure(&lighting);
    assert!((exposure - 0.75 / (0.5 + key_share)).abs() < 1.0e-3, "meters the map, the disc included: {exposure}");
    // Legacy lane: the fill exposed in the values; the sun exposed too, but
    // held within white next to that fill (hdr.color.x * exposure = 1.74 would
    // clip: no tone mapper in this lane), so a wall facing it reads 1.0.
    renderer.hdr_output = false;
    let legacy = renderer.env_sun_rig(&world, stock);
    assert!((legacy.sky.x - hdr.sky.x * exposure).abs() < 2.0e-3, "{:?}", legacy.sky);
    assert!(hdr.color.x * exposure > 1.0, "premise: this sun would clip when exposed");
    assert!((legacy.color.x + legacy.sky.x - 1.0).abs() < 5.0e-4, "{:?} {:?}", legacy.color, legacy.sky);
    assert_eq!(legacy.dir, hdr.dir);
    assert_eq!(legacy.shadow_alpha, stock.shadow_alpha);

    // An authored colour and ambient are the script's, in the lane's units.
    let mut authored = world.clone();
    authored.sun.color = Some(vec3f(0.3, 0.2, 0.1));
    authored.sun.ambient = Some(vec3f(0.05, 0.05, 0.05));
    let rig = renderer.env_sun_rig(&authored, crate::sun::resolve_sun(&authored.sun));
    assert_eq!((rig.color, rig.sky, rig.ground), (vec3f(0.3, 0.2, 0.1), vec3f(0.05, 0.05, 0.05), vec3f(0.05, 0.05, 0.05)));
    assert!(rig.dir.dot(toward) > 0.9999, "the map still places the sun");

    // The world stops declaring the sun: a new preparation starts, and the
    // bound one (the old sun filled in its lighting copy) keeps lighting.
    world.environment.sun = None;
    renderer.resolve_ibl(&mut cx, &world.environment);
    assert!(renderer.ibl_pending());
    assert_eq!(renderer.env_sun(&world), Some(sun), "one key behind the world until the new preparation lands");
    renderer.hdr_output = true;
    assert!(renderer.env_sun_rig(&world, stock.to_hdr()).color.x > 0.0);
    settle(&mut renderer, &mut cx, &world);
    assert_eq!(renderer.ibl_preparations(), 2);
    assert_eq!(renderer.env_sun(&world), None);
    assert_eq!(renderer.env_sun_rig(&world, stock.to_hdr()).color, Vec3f::default(), "no key: no direct light, never the analytic colour");
    assert!(renderer.env_sun_dir(&world).is_none(), "the shadows stay on the rig's own direction");
}

/// A prepared environment colours the host's fog and feeds the rig; an
/// authored `Fog`, an MR stage and a missing preparation leave it alone.
#[test]
fn a_prepared_environment_colours_the_host_fog_only() {
    use makepad_render_material::ibl::{sh9, EnvMap};
    let mut r = Renderer::default();
    r.set_hdr_output(true);
    // 128 wide: sh9's midpoint quadrature is 0.4 % high at 32 wide (1.5 %
    // at 16), outside the 2e-3 asserts below.
    let map = EnvMap::constant(128, [1.0, 1.0, 1.0]);
    r.feed_environment_numbers_for_tests(sh9(&map), 1.0, vec3f(0.5, 0.6, 0.7));
    let mut world = World::new();
    world.environment.ibl = Some(makepad_scene::Ibl {
        source: makepad_scene::IblSource::Hdri(makepad_scene::TextureRef(1)),
        intensity: 1.0,
        rotation_deg: 0.0,
    });
    // HDR lane: the band at the map's own scale × Ibl.intensity 1, exactly
    // (HDR_SKY_GAIN is the analytic sky's and never reaches the
    // environment's fog).
    assert_eq!(r.env_fog_color(&world, true), Some(vec3f(0.5, 0.6, 0.7)));
    assert!(r.env_fog_color(&world, false).is_none(), "MR: the room supplies the horizon");
    // Legacy lane: the fog is the legacy dome's own horizon, i.e. the band
    // through the dome's tone map at the dome's exposure (metered mean
    // 1.0 × intensity 1, no exposure_ev: EXPOSURE_KEY / 1.0).
    r.set_hdr_output(false);
    assert_eq!(
        r.env_fog_color(&world, true),
        Some(crate::sun::legacy_dome_rgb(vec3f(0.5, 0.6, 0.7), crate::sky::EXPOSURE_KEY)),
        "the legacy fog meets the legacy dome"
    );
    // At intensity 2 the dome shows twice the radiance and meters twice the
    // mean: the legacy fog is the band × 2 at EXPOSURE_KEY / 2, as the dome.
    world.environment.ibl.as_mut().unwrap().intensity = 2.0;
    assert_eq!(
        r.env_fog_color(&world, true),
        Some(crate::sun::legacy_dome_rgb(vec3f(0.5, 0.6, 0.7) * 2.0, crate::sky::EXPOSURE_KEY / 2.0))
    );
    world.environment.ibl.as_mut().unwrap().intensity = 1.0;
    r.set_hdr_output(true);
    world.environment.fog = makepad_scene::Fog::Exp2 { color: vec3f(1.0, 1.0, 1.0), density: 0.01 };
    assert!(r.env_fog_color(&world, true).is_none(), "an authored fog keeps its colour");
    world.environment.fog = makepad_scene::Fog::Host;
    // The same preparation feeds the rig: a white map, no sun -> fill only,
    // E/π = 1.0 at the map's own scale.
    let rig = r.env_sun_rig(&world, SunLight::default().to_hdr());
    assert_eq!(rig.color, Vec3f::default());
    assert!((rig.sky.x - 1.0).abs() < 2.0e-3, "{:?}", rig.sky);
    // Intensity is the gain, so fill and fog move together.
    world.environment.ibl.as_mut().unwrap().intensity = 2.0;
    assert_eq!(r.env_fog_color(&world, true), Some(vec3f(0.5, 0.6, 0.7) * 2.0));
    assert!((r.env_sun_rig(&world, SunLight::default().to_hdr()).sky.x - 2.0).abs() < 2.0e-3);
    // A non-finite intensity is no environment at all.
    world.environment.ibl.as_mut().unwrap().intensity = f32::NAN;
    assert!(r.env_fog_color(&world, true).is_none());
    assert_eq!(r.env_sun_rig(&world, SunLight::default()), SunLight::default());
    // The meter counts the environment's sun: the preparation metered the
    // lighting copy (cone filled, 1.0 here), env_lighting adds the sun's
    // share of the sphere mean, L (1 - cos r) / 2 = 1e4 x 1e-4 / 2 = 0.5.
    world.environment.ibl.as_mut().unwrap().intensity = 1.0;
    world.environment.sun = Some(makepad_scene::EnvSun {
        dir: vec3f(0.0, 1.0, 0.0),
        radiance: vec3f(1.0e4, 1.0e4, 1.0e4),
        cos_radius: 0.9999,
        facing: 1.0,
        cos_cover: 0.9999,
    });
    let metered = r.env_lighting(&world).unwrap().mean_luminance;
    assert!(metered > 1.4 && metered < 1.6, "1.0 + 0.5, got {metered}");
    assert!(crate::sun::env_exposure(&r.env_lighting(&world).unwrap()) < crate::sun::env_exposure(&crate::sun::EnvLighting { mean_luminance: 1.0, ..r.env_lighting(&world).unwrap() }), "a sunny map meters darker than its sky alone");
    // And the rig lights with that sun (declared here; a procedural hdri
    // preset's baked sun arrives the same way through `env_sun`).
    assert_eq!(r.env_sun(&world), world.environment.sun);
    assert!(r.env_sun_rig(&world, SunLight::default().to_hdr()).color.x > 0.0);
}

/// The fog is the dome's horizon, so it takes what the dome takes: the
/// background's own intensity (the shader multiplies it in after the IBL's)
/// in both lanes; and with no preparation landed there is no environment
/// fog at all, whatever the world names.
#[test]
fn the_host_fog_takes_the_backgrounds_intensity_and_waits_for_the_preparation() {
    let mut world = World::new();
    world.environment.ibl = Some(makepad_scene::Ibl {
        source: makepad_scene::IblSource::Hdri(makepad_scene::TextureRef(1)),
        intensity: 2.0,
        rotation_deg: 0.0,
    });
    let mut r = Renderer::default();
    r.set_hdr_output(true);
    assert!(r.env_fog_color(&world, true).is_none(), "nothing prepared yet: the host's fog stands");
    let sh = makepad_render_material::ibl::sh9(&makepad_render_material::ibl::EnvMap::constant(32, [1.0, 1.0, 1.0]));
    r.feed_environment_numbers_for_tests(sh, 1.0, vec3f(0.5, 0.6, 0.7));
    let band = vec3f(0.5, 0.6, 0.7);
    // A background that is not the environment shows no intensity of its own.
    assert_eq!(r.env_fog_color(&world, true), Some(band * 2.0));
    world.environment.background = makepad_scene::Background::Environment { blur: 0.0, intensity: 0.5 };
    assert_eq!(r.env_fog_color(&world, true), Some(band * 0.5 * 2.0), "HDR: the dome's radiance x Ibl.intensity x the background's");
    // Legacy: the same radiance through the dome's tone map, metered on the
    // map's mean x Ibl.intensity alone (what draw_environment_background
    // passes; the background's intensity is not part of the meter).
    r.set_hdr_output(false);
    assert_eq!(
        r.env_fog_color(&world, true),
        Some(crate::sun::legacy_dome_rgb(band * 0.5 * 2.0, crate::sky::EXPOSURE_KEY / 2.0))
    );
    // The sky's exposure compensation moves the dome's exposure, so the fog's.
    world.sky = Some(makepad_scene::SkyConfig { exposure_ev: 1.0, ..Default::default() });
    assert_eq!(
        r.env_fog_color(&world, true),
        Some(crate::sun::legacy_dome_rgb(band * 0.5 * 2.0, crate::sky::EXPOSURE_KEY / 2.0 * 2.0))
    );
}
