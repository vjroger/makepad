//! Stock PBR props under image-based lighting (phase 3 of the HDRI plan).
//!
//! A shiny placed model with no material of its own draws on the PBR lane
//! (`ModelDraw::Pbr`) until a world names an environment. Then it draws
//! through `__stock_ibl`: an engine-owned Splash material program that is
//! the PBR lane with `Builtin::Ibl` installed (custom_material.rs) and
//! nothing else — no hooks, the stock shadow caster, nothing that cuts a
//! pixel (its opaque draws take the variant without `clip`, as the PBR
//! lane's do). `ModelDraw::Custom` walks the same instance loop as the PBR
//! lane, so the model keeps its textures (bound by name), its morph
//! targets and its generated skin surface; slot 5 receives the atlas as it
//! does for every IBL material (draw_models.rs).
//!
//! The program is built on first use, once, and stays installed. Whether
//! this frame routes through it is decided after the environment is
//! resolved (`prepare_stock_ibl`, in `draw_scene_scoped` after
//! `resolve_ibl_for`), from the IBL texture (the bound preparation's; a
//! fork's is the live renderer's, mirrored) and the shader on the
//! program's draw, and decided again in each model list's diffuse pass,
//! before any lane walks that list (`confirm_stock_ibl`, beside
//! `pbr_ready`): only there are this frame's features known, and the lanes
//! draw through the program's variant for them, a shader of its own, so the
//! models leave the PBR lane only once that variant's pipeline is ready.
//! Without an environment nothing is built and both lanes take exactly the
//! instances they took before.
use super::*;
use crate::custom_material::DrawSceneCustom;
use makepad_render_material::{HookMask, HookSet, MaterialDesc};

/// The custom-material name the stock IBL program is installed under. An
/// engine name (the `__item_*` convention): a host never installs it.
pub const STOCK_IBL_MATERIAL: &str = "__stock_ibl";

#[derive(Default)]
pub(super) struct StockIblState {
    /// The program did not build (logged once, never retried).
    failed: bool,
    /// This frame routes shiny stock models through the program.
    active: bool,
}

impl Renderer {
    /// Install `__stock_ibl` when it is missing (and did not fail before).
    /// True when it is installed after the call. A held VM (a script-driven
    /// draw mid-apply) means "next frame", not failure.
    pub(super) fn stock_ibl_install(&mut self, cx: &mut Cx) -> bool {
        if self.custom_material_shader(STOCK_IBL_MATERIAL).is_some() {
            return true;
        }
        if self.stock_ibl.failed {
            return false;
        }
        // The PBR lane with IBL and nothing else: the stock bodies of every
        // hook, so a recoloured car keeps its clear coat and its maps.
        let desc = MaterialDesc { ibl: true, ..Default::default() };
        match cx.try_with_vm(|vm| DrawSceneCustom::build(vm, &desc, &HookSet::new(), HookMask::ALL, Vec4f::default())) {
            Some(Ok(material)) => {
                // A program the lane cannot instance is a failure too:
                // retried, it would be built again on every frame.
                let installed = self.install_custom_material(STOCK_IBL_MATERIAL.to_string(), material);
                if !installed {
                    log!("render: the stock IBL material has no shader, shiny models keep the analytic sky");
                    self.stock_ibl.failed = true;
                }
                installed
            }
            Some(Err(e)) => {
                log!("render: the stock IBL material did not build, shiny models keep the analytic sky: {e}");
                self.stock_ibl.failed = true;
                false
            }
            None => false,
        }
    }

    /// Decide, once per frame after the environment is resolved, whether
    /// shiny stock models draw through the program this frame: the PBR lane
    /// is enabled, an IBL texture is bound (the bound preparation's, which
    /// stays while the next prepares; a fork's, mirrored from the live
    /// renderer) and the shader on the program's draw is ready. Without an
    /// environment the program is never built: the short-circuit is the
    /// no-change guarantee. The shader on the draw is the variant the last
    /// list decided from (the stock shader before the first), not
    /// necessarily the one this frame's lanes draw through: this frame's
    /// features are known only once the lights are clustered, later in the
    /// frame. So each list's diffuse pass decides again (`confirm_stock_ibl`)
    /// before any lane walks that list.
    pub(super) fn prepare_stock_ibl(&mut self, cx: &mut Cx) {
        self.stock_ibl.active = self.pbr_materials_enabled
            && self.ibl_texture().is_some()
            && self.stock_ibl_install(cx)
            && self.stock_ibl_shader_ready(cx);
    }

    /// Decide again in a model list's diffuse pass, beside `pbr_ready`
    /// (draw_models.rs) and before any lane walks the list, now that the
    /// frame's features are known: the PBR lane is enabled, an IBL texture
    /// is bound, the program is installed and its variant for this frame's
    /// features (`CustomMaterial::shaders`, built on first use: a shader of
    /// its own, whose pipeline Metal compiles apart) is ready. Otherwise the
    /// models stay on the PBR lane, as `pbr_ready` keeps them on the
    /// diffuse lane while the PBR lane's variant compiles. The variant goes
    /// on the program's draw, where the program's own pass puts it too, so
    /// `stock_ibl_ready` (`items_ready`) reads the shader the lane draws
    /// through. Installs nothing: a program `prepare_stock_ibl` did not
    /// install is no route.
    pub(super) fn confirm_stock_ibl(&mut self, cx: &mut Cx) {
        if !self.pbr_materials_enabled || self.ibl_texture().is_none() {
            self.stock_ibl.active = false;
            return;
        }
        let (features, hdr) = (self.lane_features(), self.hdr_output);
        self.stock_ibl.active = self.custom_draws.get_mut(STOCK_IBL_MATERIAL).is_some_and(|m| {
            let full = m.shaders(cx, features).0;
            m.draw.draw_vars.draw_shader_id = full;
            full.is_some_and(|id| cx.draw_shader_ready(id, hdr))
        });
    }

    /// This frame's decision (`prepare_stock_ibl`, then the list's diffuse
    /// pass: `confirm_stock_ibl`), read by the lanes.
    pub(super) fn stock_ibl_active(&self) -> bool {
        self.stock_ibl.active
    }

    /// Whether a locked-time host has nothing to wait for on this path: no
    /// environment, the PBR lane off, no program installed, or the pipeline
    /// of the shader on the program's draw ready, the variant the lanes
    /// last drew through (`items_ready`). Not installed is nothing
    /// to wait for, as for the items' materials and the PBR lane there: the
    /// scene draw that binds an environment installs the program before any
    /// lane walks the models, so after a draw it is missing only when it did
    /// not build (those models stay on the PBR lane for good) or the VM was
    /// held (as a held VM leaves the PBR lane unmade).
    pub(super) fn stock_ibl_ready(&self, cx: &Cx) -> bool {
        self.ibl_texture().is_none()
            || !self.pbr_materials_enabled
            || !self.custom_draws.contains_key(STOCK_IBL_MATERIAL)
            || self.stock_ibl_shader_ready(cx)
    }

    /// Whether the shader on the program's draw can draw: the variant the
    /// last diffuse pass decided from (`confirm_stock_ibl` puts it there,
    /// the program's own pass the same one), so within a list the shader
    /// the lanes draw through, and the stock shader before the first
    /// diffuse pass. Read before the walk it is the last list's variant, a
    /// first estimate only (`prepare_stock_ibl`).
    fn stock_ibl_shader_ready(&self, cx: &Cx) -> bool {
        self.custom_draws
            .get(STOCK_IBL_MATERIAL)
            .and_then(|m| m.draw.draw_vars.draw_shader_id)
            .is_some_and(|id| cx.draw_shader_ready(id, self.hdr_output))
    }
}

/// Whether one instance draws through the stock IBL program this frame:
/// the frame routes (`active`), the model wants the PBR lane, it names no
/// material of its own and it is not swaying foliage (the foliage lane keeps
/// those). Read by every lane, so exactly one of them draws the instance.
pub(super) fn takes_stock_ibl(active: bool, uses_pbr_lane: bool, own_material: bool, sways: bool) -> bool {
    active && uses_pbr_lane && !own_material && !sways
}

/// Whether a stock lane (diffuse, PBR or foliage) skips one instance. With
/// `takes_stock_ibl` false this is the filter the lanes had before.
pub(super) fn stock_lane_skips(wanted_custom: bool, takes_stock_ibl: bool, sways: bool, foliage_lane: bool, uses_pbr_lane: bool, pbr_lane: bool) -> bool {
    wanted_custom || takes_stock_ibl || sways != foliage_lane || (!foliage_lane && uses_pbr_lane != pbr_lane)
}

#[cfg(test)]
mod tests {
    use super::*;
    use makepad_draw::makepad_platform::makepad_script::script_eval;
    use makepad_render_material::{Builtin, ShadowVariant};

    /// A headless Cx whose VM holds the scene shaders and the material
    /// built-ins (the custom_material.rs test rig).
    fn headless() -> Cx {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            vm.bx.captured_errors = Some(Vec::new());
            makepad_draw::script_mod(vm);
            vm.bx.heap.new_module(id!(prelude));
            script_eval!(vm, { mod.prelude.widgets_internal = { ..mod.std, ..mod.pod, ..mod.math, ..mod.sdf, ..mod.shader, draw:mod.draw } });
            vm.bx.heap.new_module(id!(widgets));
            crate::custom_material::register(vm);
        });
        cx
    }

    #[test]
    fn only_a_shiny_stock_model_under_an_environment_takes_the_program() {
        // (active, uses_pbr_lane, own_material, sways)
        assert!(takes_stock_ibl(true, true, false, false));
        assert!(!takes_stock_ibl(false, true, false, false), "no environment: the PBR lane");
        assert!(!takes_stock_ibl(true, false, false, false), "a matte model: the diffuse lane");
        assert!(!takes_stock_ibl(true, true, true, false), "its own material: that program");
        assert!(!takes_stock_ibl(true, true, false, true), "swaying: the foliage lane");
    }

    /// The composed lane filter of draw_models_inner, as a truth table: a
    /// stock instance (no material of its own) is drawn by exactly one of
    /// the diffuse, PBR and foliage lanes or the stock IBL program, and
    /// with no environment the filter is the expression the lanes had.
    #[test]
    fn exactly_one_lane_draws_a_stock_instance_and_no_environment_is_the_old_filter() {
        // (foliage_lane, pbr_lane) of the diffuse, PBR and foliage lanes.
        let lanes = [(false, false), (false, true), (true, true)];
        for bits in 0..16u32 {
            let (active, uses_pbr, own, sways) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            let takes = takes_stock_ibl(active, uses_pbr, own, sways);
            let stock = lanes.iter().filter(|(foliage, pbr)| !stock_lane_skips(false, takes, sways, *foliage, uses_pbr, *pbr)).count();
            assert_eq!(stock + takes as usize, 1, "active {active} pbr {uses_pbr} own {own} sways {sways}");
            if !active {
                for (foliage, pbr) in lanes {
                    let before = sways != foliage || (!foliage && uses_pbr != pbr);
                    assert_eq!(stock_lane_skips(false, takes, sways, foliage, uses_pbr, pbr), before, "no environment: the lanes' old filter");
                }
            }
            // An instance whose own material is ready leaves every stock lane.
            for (foliage, pbr) in lanes {
                assert!(stock_lane_skips(true, takes, sways, foliage, uses_pbr, pbr), "a ready material of its own: no stock lane");
            }
        }
    }

    #[test]
    fn the_program_is_the_pbr_lane_with_ibl_and_nothing_else() {
        let mut cx = headless();
        let mut renderer = Renderer::default();
        assert!(renderer.stock_ibl_install(&mut cx), "the program builds and installs");
        assert!(renderer.stock_ibl_install(&mut cx), "installed once: the second call finds it");
        let m = renderer.custom_material(STOCK_IBL_MATERIAL).expect("installed under its name");
        assert!(m.ibl, "binds the environment on detail_map");
        assert_eq!(m.plan.builtins, vec![Builtin::Ibl], "no hooks: no hooked composition");
        assert_eq!(m.plan.shadow, ShadowVariant::Stock);
        assert!(m.shadow.is_none(), "the stock caster");
        assert!(m.cuts_no_pixel(&cx), "nothing cuts a pixel: its opaque draws take the variant without `clip`");
        assert_eq!(m.bounds_pad, 0.0);
        let id = m.stock_shader().expect("compiled");
        let textures = &cx.draw_shaders[id.index].mapping.textures;
        assert_eq!(textures[0].id, live_id!(tex));
        assert_eq!(textures[5].id, live_id!(detail_map), "the PBR lane's free detail slot");
        assert!(textures.iter().any(|t| t.id == live_id!(orm_map)), "a stock model's ORM binds as on the PBR lane");
        assert!(textures.iter().any(|t| t.id == live_id!(morph_map)), "its morph targets too");
        assert!(renderer.stock_ibl_ready(&cx));
    }

    #[test]
    fn without_an_environment_nothing_is_built_and_the_frame_routes_as_before() {
        let mut cx = headless();
        let mut renderer = Renderer::default();
        renderer.prepare_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active());
        assert!(renderer.custom_material(STOCK_IBL_MATERIAL).is_none(), "no environment, no program");
        assert!(renderer.custom_draws.is_empty());
        assert!(renderer.stock_ibl_ready(&cx), "nothing to wait for");
        for (uses_pbr, own, sways) in [(true, false, false), (true, true, false), (false, false, false), (true, false, true)] {
            assert!(!takes_stock_ibl(renderer.stock_ibl_active(), uses_pbr, own, sways));
        }
    }

    /// Under a bound environment: before a scene draw has installed the
    /// program there is nothing to wait for (plan 2's environment tests bind
    /// a texture with no scene draw and expect `items_ready`), and with the
    /// PBR lane off the frame neither builds the program nor routes.
    #[test]
    fn under_an_environment_the_pbr_lane_gates_the_program_and_no_draw_yet_waits_for_nothing() {
        use makepad_draw::makepad_platform::thread::ShutdownMode;
        use makepad_render_material::ibl::EnvMap;
        use makepad_scene::{Environment, Ibl, IblSource, TextureRef};
        let mut cx = headless();
        // A closed pool prepares on the spot: the call binds the texture.
        cx.task_pool().close(ShutdownMode::CancelPending);
        let mut renderer = Renderer::default();
        renderer.register_environment(TextureRef(1), std::sync::Arc::new(EnvMap::constant(16, [0.25; 3])));
        let env = Environment { ibl: Some(Ibl { source: IblSource::Hdri(TextureRef(1)), intensity: 1.0, rotation_deg: 0.0 }), ..Default::default() };
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.ibl_texture().is_some(), "premise: the environment is bound");
        assert!(renderer.stock_ibl_ready(&cx) && renderer.items_ready(&cx), "no draw has installed the program: nothing to wait for");

        renderer.pbr_materials_enabled = false;
        renderer.prepare_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active(), "the PBR lane off: no routing");
        assert!(renderer.custom_material(STOCK_IBL_MATERIAL).is_none(), "and no program");

        // The control: the same frame with the lane on builds the program
        // and routes once its pipeline can draw.
        renderer.pbr_materials_enabled = true;
        renderer.prepare_stock_ibl(&mut cx);
        let id = renderer.custom_material_shader(STOCK_IBL_MATERIAL).expect("the lane on: the program is built");
        let active = renderer.stock_ibl_active();
        assert_eq!(active, cx.draw_shader_ready(id, renderer.hdr_output), "the decision is the program's pipeline");
        // Every backend but Metal compiles synchronously (window_snapshot.rs).
        #[cfg(not(target_vendor = "apple"))]
        assert!(active, "the lane on and a ready program: the frame routes");
    }

    /// The lanes draw the program through its variant for this frame's
    /// features, a shader of its own (`CustomMaterial::shaders`), not the
    /// one `prepare_stock_ibl` reads off the draw before the walk (the
    /// stock shader, or last walk's variant). A list's diffuse pass decides
    /// again from that variant and puts it on the draw, so the lanes,
    /// `stock_ibl_ready` and `items_ready` read one shader. On Metal a
    /// decision read off the other shader routed the models while their
    /// variant compiled: the PBR lane had left them and the variant's draw
    /// was left out of the frame.
    #[test]
    fn each_list_decides_from_this_frames_variant_and_puts_it_on_the_draw() {
        use makepad_draw::makepad_platform::thread::ShutdownMode;
        use makepad_render_material::ibl::EnvMap;
        use makepad_scene::{Environment, Ibl, IblSource, TextureRef};
        let mut cx = headless();
        cx.task_pool().close(ShutdownMode::CancelPending);
        let mut renderer = Renderer::default();
        let on_draw = |r: &Renderer| r.custom_material(STOCK_IBL_MATERIAL).and_then(|m| m.draw.draw_vars.draw_shader_id);

        // No environment: the diffuse pass builds nothing and routes nothing.
        renderer.confirm_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active());
        assert!(renderer.custom_draws.is_empty(), "no environment, no program");

        renderer.register_environment(TextureRef(1), std::sync::Arc::new(EnvMap::constant(16, [0.25; 3])));
        let env = Environment { ibl: Some(Ibl { source: IblSource::Hdri(TextureRef(1)), intensity: 1.0, rotation_deg: 0.0 }), ..Default::default() };
        renderer.resolve_ibl(&mut cx, &env);
        assert!(renderer.ibl_texture().is_some(), "premise: the environment is bound");
        renderer.prepare_stock_ibl(&mut cx);
        let stock = renderer.custom_material_shader(STOCK_IBL_MATERIAL).expect("an environment builds the program");
        assert_eq!(on_draw(&renderer), Some(stock), "before its first walk the draw holds the stock shader");

        renderer.confirm_stock_ibl(&mut cx);
        let features = renderer.lane_features();
        let variant = renderer.custom_draws.get_mut(STOCK_IBL_MATERIAL).unwrap().shaders(&mut cx, features).0.expect("a variant for this frame's features");
        assert_ne!(variant, stock, "premise: this frame's variant is a shader of its own");
        assert_eq!(on_draw(&renderer), Some(variant), "the variant the lanes draw through is on the draw");
        let active = renderer.stock_ibl_active();
        assert_eq!(active, cx.draw_shader_ready(variant, renderer.hdr_output), "the decision is that variant's pipeline");
        // Every backend but Metal compiles synchronously (window_snapshot.rs).
        #[cfg(not(target_vendor = "apple"))]
        assert!(active, "an environment and a ready variant: the frame routes");
        assert_eq!(renderer.stock_ibl_ready(&cx), active, "items_ready waits on the shader the lanes draw through");

        // The next frame's first decision reads the variant off the draw;
        // another list's diffuse pass finds it built and decides the same.
        renderer.prepare_stock_ibl(&mut cx);
        assert_eq!(renderer.stock_ibl_active(), active);
        renderer.confirm_stock_ibl(&mut cx);
        assert_eq!(on_draw(&renderer), Some(variant), "one variant per feature set, built once");
        assert_eq!(renderer.stock_ibl_active(), active);

        // The PBR lane off, or the environment gone: the diffuse pass does
        // not route either.
        renderer.pbr_materials_enabled = false;
        renderer.confirm_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active(), "the PBR lane off: no routing");
        renderer.pbr_materials_enabled = true;
        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert!(renderer.ibl_texture().is_none());
        renderer.confirm_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active(), "no environment: no routing");
    }

    use crate::model::PbrMaterial;

    /// A resident stock model in the fixture form of
    /// prepared_static_preview_tests.rs: one triangle, seven floats a vertex.
    fn triangle(pbr: PbrMaterial) -> StaticModel {
        let mut vertices = Vec::new();
        for p in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] {
            vertices.extend_from_slice(&[p[0], p[1], p[2], 0.0, 0.0, f32::from_bits(0xffff_ffff), 0.0]);
        }
        StaticModel {
            vertices, indices: vec![0, 1, 2], texture_uri: None, texture_png: None,
            min: vec3f(0.0, 0.0, 0.0), max: vec3f(1.0, 0.0, 1.0),
            parts: vec![(vec3f(0.0, 0.0, 0.0), vec3f(1.0, 0.0, 1.0))],
            ground_ao: None, draw_layers: Vec::new(), detail_png: None, detail_scale: [1.0, 1.0],
            prelit: false, anim_parts: Vec::new(), driven_parts: Vec::new(), sky: None, liquids: Vec::new(), liquid_ranges: Vec::new(),
            pbr,
        }
    }

    /// Whether the resident model `name` asks for the PBR lane.
    fn wants_pbr(renderer: &Renderer, name: &str) -> bool {
        renderer.static_models.iter().find(|(k, _)| k == name).map(|(_, m)| m.wants_pbr).unwrap()
    }

    /// A renderer with a chrome and a matte stock model resident and placed.
    fn renderer_with_a_chrome_and_a_matte_model(cx: &mut Cx) -> Renderer {
        let mut renderer = Renderer::default();
        let chrome = PbrMaterial { metallic: 1.0, roughness: 0.15, ..Default::default() };
        renderer.load_model_parsed(cx, "test/chrome", triangle(chrome), None, None).expect("the chrome model uploads");
        renderer.load_model_parsed(cx, "test/matte", triangle(PbrMaterial::default()), None, None).expect("the matte model uploads");
        assert!(wants_pbr(&renderer, "test/chrome") && !wants_pbr(&renderer, "test/matte"), "a narrowed roughness is the PBR lane, the default the diffuse lane");
        let instance = |model: &str| ModelInstance { model: model.into(), custom_material: None, transform: Mat4f::identity(), tint: vec4(1.0, 1.0, 1.0, 1.0), color_adjust: vec4(0.0, 1.0, 1.0, 0.0), dynamic: true, depth_order: 0.0, part_poses: Vec::new() };
        renderer.set_models(vec![instance("test/chrome"), instance("test/matte")]);
        renderer
    }

    #[test]
    fn placed_stock_models_without_an_environment_keep_their_lanes() {
        let mut cx = headless();
        let mut renderer = renderer_with_a_chrome_and_a_matte_model(&mut cx);
        // The frame's decision with nothing registered: the environment is
        // absent, so no program exists and neither model changes lane.
        renderer.prepare_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active());
        assert!(renderer.custom_draws.is_empty(), "no environment: not even the program is built");
        // The lanes' filter for both models is the one they had before
        // (the diffuse, PBR and foliage lanes as (foliage_lane, pbr_lane)).
        let sways = false;
        for name in ["test/chrome", "test/matte"] {
            let uses_pbr = wants_pbr(&renderer, name);
            let takes = takes_stock_ibl(renderer.stock_ibl_active(), uses_pbr, false, sways);
            assert!(!takes, "{name} stays on its lane");
            for (foliage, pbr) in [(false, false), (false, true), (true, true)] {
                let before = sways != foliage || (!foliage && uses_pbr != pbr);
                assert_eq!(stock_lane_skips(false, takes, sways, foliage, uses_pbr, pbr), before, "{name}: lane ({foliage}, {pbr})");
            }
        }
        assert!(renderer.items_ready(&cx), "and nothing is waited for");
    }

    /// The active path, headless: an environment's lane texture turns the
    /// frame's decision on, the program is built once, only the shiny
    /// model leaves the PBR lane for it, and dropping the environment
    /// turns the decision off again.
    #[test]
    fn an_environment_routes_the_shiny_stock_model_and_only_it() {
        use makepad_render_material::ibl::EnvMap;
        use makepad_scene::{Environment, Ibl, IblSource, TextureRef};
        let mut cx = headless();
        let mut renderer = renderer_with_a_chrome_and_a_matte_model(&mut cx);
        renderer.register_environment(TextureRef(1), std::sync::Arc::new(EnvMap::constant(32, [0.5, 0.4, 0.3])));
        let mut env = Environment::default();
        env.ibl = Some(Ibl { source: IblSource::Hdri(TextureRef(1)), intensity: 1.0, rotation_deg: 0.0 });
        // resolve_ibl prepares the lane texture on a background job (plan 2:
        // one job at a time, adopted by the resolve after it lands; a closed
        // pool prepares on the calling thread, small): poll it as a frame does.
        let start = std::time::Instant::now();
        while renderer.ibl_texture().is_none() {
            renderer.resolve_ibl(&mut cx, &env);
            assert!(start.elapsed().as_secs() < 180, "the environment never prepared");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        renderer.prepare_stock_ibl(&mut cx);
        let id = renderer.custom_material_shader(STOCK_IBL_MATERIAL).expect("an environment builds the program");
        let active = renderer.stock_ibl_active();
        assert_eq!(active, cx.draw_shader_ready(id, renderer.hdr_output), "the decision is the texture plus the program's pipeline");
        // Every backend but Metal compiles synchronously (window_snapshot.rs).
        #[cfg(not(target_vendor = "apple"))]
        assert!(active, "an environment and a ready program: the frame routes");
        let (chrome, matte) = (wants_pbr(&renderer, "test/chrome"), wants_pbr(&renderer, "test/matte"));
        assert_eq!(takes_stock_ibl(active, chrome, false, false), active, "the chrome model takes the program");
        assert!(!takes_stock_ibl(active, matte, false, false), "the matte model keeps the diffuse lane");
        // Exactly one drawer for each: the program or one stock lane.
        for uses_pbr in [chrome, matte] {
            let takes = takes_stock_ibl(active, uses_pbr, false, false);
            let stock = [(false, false), (false, true), (true, true)].iter().filter(|(foliage, pbr)| !stock_lane_skips(false, takes, false, *foliage, uses_pbr, *pbr)).count();
            assert_eq!(stock + takes as usize, 1);
        }
        assert_eq!(renderer.stock_ibl_ready(&cx), active, "items_ready waits exactly while the pipeline compiles");
        // A second frame finds the program installed: one program, built once.
        renderer.prepare_stock_ibl(&mut cx);
        assert_eq!(renderer.custom_material_shader(STOCK_IBL_MATERIAL), Some(id));
        assert_eq!(renderer.custom_draws.len(), 1);
        // No environment again: the decision is off, both models are back
        // on their lanes; the program stays installed for the next one.
        renderer.resolve_ibl(&mut cx, &Environment::default());
        assert!(renderer.ibl_texture().is_none());
        renderer.prepare_stock_ibl(&mut cx);
        assert!(!renderer.stock_ibl_active());
        assert!(!takes_stock_ibl(renderer.stock_ibl_active(), chrome, false, false));
        assert!(renderer.custom_material(STOCK_IBL_MATERIAL).is_some());
    }
}
