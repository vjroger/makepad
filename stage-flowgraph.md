# Makepad Stage: composable workflow apps

Implementation plan · 2026-09-20 · Incorporates Fable's final review

## TL;DR

Build Stage from a **hierarchy of reusable Splash flows**, using a shared engine that can also run inside Sandbox. AI iterates the graph and its UI; the result is shareable as one workflow app.

- Make Stage's audio, video, VJ, browser, GenAI, lighting and controller systems wireable components. Replace Stage's asset-creator orchestration with Flow.
- Use one graph model for generation, live performance and editing. Outputs go to result, asset, playback, recording or streaming bins. Entire mixes, scenes and edits can become reusable subflows.
- **Audio must not drop blocks. Video may drop frames. All GPU rendering stays on the main thread.** Workers handle decoding, analysis, compilation and CPU preparation.
- Support generated controls, MIDI/OSC, AI decisions, ordinary Splash code, animated wires and probes through shared typed interfaces.
- First prove the shared engine in Sandbox and the webcam → capture → image-to-video → live VJ example. Then migrate the remaining systems and deliver editing, streaming and app packaging.

The [implementation sequence](#step-by-step-implementation-plan) at the bottom is the work order. The architecture below defines the constraints each step must preserve.

## Product and scope

Stage becomes a host and editor for workflows assembled from existing components. A workflow app contains its entry flows, reusable modules, generated UI, defaults and declared resource requirements. It can run, be edited with AI, and be shared without flattening its source into one large Splash script.

The same engine is embeddable in other applications. Sandbox supplies its own content-creation UI, asset services and nodes; it does not depend on Stage's shell, device setup or graph window.

A finite result is an output-bin use case, not a separate graph type. Persistent streams and triggered jobs can coexist, and several flow instances can run concurrently. Closing an inspector or selecting another graph does not stop them.

Reference workflow:

```text
Webcam → Capture frame → Image-to-video → Prepared clip → Layer/scene → Program
                              └────────────────────────→ Asset-database bin
```

Generation and publication must not interrupt the current program. Playback can start when a clip is ready, independently of asset publication. The scene is itself reusable inside another flow.

## Architecture decisions

### Shared engine and reusable providers

| Layer | Responsibility |
|---|---|
| Shared engine, based on libs/flow | Documents, module resolution, registry, validation, instances, activations, revisions, bounded commands/observations and execution-provider contracts. |
| Reusable providers and services | Audio/video implementations, codecs, asset access, GenAI, devices and plugin hosting. Concrete device schedules and media resource types stay outside the generic engine. |
| Shared authoring tools | Existing libs/flowgraph canvas plus reusable controller, face and binding code extracted from apps/flow-ui. Views are optional consumers of the engine. |
| Application hosts | Stage, Sandbox and other apps supply services, supported providers, local resource bindings and UI. They own running instances explicitly. |

Use an in-process embedding API; an HTTP server or Stage process is not required. Start with clear modules and feature boundaries in existing shared libraries. Add crates where they establish a real reusable boundary, including the requested **libs/external_audio**. Do not create a large crate hierarchy before it is needed.

One descriptor registry defines node interfaces, parameters, execution requirements and capabilities. It drives the loader, writer, editor and AI catalog. Keep execution ownership separate from the existing AI-provider meaning of Node.domain.

### Hierarchical definitions and source ownership

- Components have stable typed inputs, outputs and exposed controls. Modules reference pinned component revisions; instances have independent state and qualified identities.
- Nested scenes, mixers, effect racks and edit flows use the same composition mechanism. Reject recursive definitions and unbounded expansion.
- Fanout shares one producer's work. Two previews of a scene do not advance it twice or reopen its devices.
- Named clocks, signals and buses resolve to real typed dependencies with explicit scope and imports/exports. The editor can reveal their wires and jump to providers; runtime execution uses resolved handles.
- Save separate modules and a small root composition. Validate interface changes and report affected connections instead of silently dropping them.
- Preserve source-owned Splash modules. Use validated source-span edits where supported; otherwise edit the source. Do not canonicalize arbitrary code after a graph drag. Converting generated topology into an editable graph is an explicit operation.
- Unsupported node/module serialization fails clearly. Extend Flow's closed reader/writer cases together; a new prototype alone is insufficient.

### Execution, timing and overload

| Work | Owner and rule |
|---|---|
| Audio DSP and sample-timed controls | Dedicated realtime executor, normally the device callback. One device schedule combines active flow partitions. Preallocated state/buffers; no blocking, allocation, script evaluation or unbounded destruction. |
| Video selection and GPU rendering | A bounded phase of the normal main-thread frame. All GPU resource operations, effect rendering and output submission stay here. Drop obsolete frames/work rather than build a backlog. |
| Decode, analysis and CPU preparation | Persistent workers prepare media, scene data and replacement plans. No image/video decode in the main-thread rendering path. |
| Control-rate Splash | Persistent, budgeted isolate per graph instance, with one serialized worker owner. Do not reload a VM per tick or evaluate control logic in the UI loop. |
| Generation, decision models and network I/O | Bounded asynchronous jobs/sessions. They publish ready results and never gate audio or rendering. |
| UI and probes | Send commands and observe bounded snapshots. Their lifetime does not own processing. |

**Audio continuity takes priority.** Essential audio buffers stay ordered; no latest-only/drop-oldest policy applies to playback or required recording audio. Prebuffer variable-latency sources and retain existing playback until replacements are ready. Source or sink failures must be reported; filling an output block with a declared fallback does not make lost source audio a success. Analysis and probe observations may be discarded without discarding the audio stream.

Video follows media timestamps, usually the audio clock. Dropping a frame does not shorten the timeline. When hardware is slow, reduce video/preview work and expose actual frame rate and drops. Extra threads do not isolate a shared GPU; no GPU-threading refactor is planned.

Every boundary has finite capacity and a defined overload policy. UI commands use nonblocking sends and retry/report when full. Prepare topology changes off the critical paths; reserve resources before activation. Audio switches at a scheduled boundary without waiting for the main thread. Video uses the matching prepared revision when it next renders. Preserve unchanged flow state and retire old resources only after their owners release them.

### Activation, routing and clocks

Output bins determine demand: result, asset database, program/cue audio, video output, recording and streaming are different terminals of the same graph. Job completion and instance lifetime are separate.

Implement **lazy branch activation** in the shared scheduler. A selector after two generators currently does not prevent both from running. Resolve the selection first, activate only the required branch, and retain producers still demanded by other consumers. Live routes separately declare whether inactive inputs continue, prewarm or suspend. Preserve existing Flow auto-archive behavior without double publication.

Triggers snapshot inputs and use explicit concurrency, queue and cancellation rules. Flow's current unbounded run queue must gain finite admission limits. Late results carry activation/revision identities and cannot overwrite unrelated current state.

Keep these separate:

- Parameter changes, with declared smoothing and timing.
- Selection/crossfade among prepared routes.
- Structural rewiring or module replacement, using a prepared revision.

Fast dial movements use prepared routes; they do not compile topology repeatedly.

Capture → BPM/beat analysis → Clock → loops, sequencers and transitions are independent components. Clocks carry tempo, phase, position and time identity, with explicit source selection. Multiple named clocks are supported. Video loops are component state, not an excuse to bypass cycle validation. Graph feedback requires an explicit delay/state contract.

### Media, GenAI and growing assets

Expose all existing job and realtime GenAI capabilities through the registry. A camera, clip or VJ output can feed live processing or a captured/segmented generation job, then return to playback, a completed asset or a growing asset. Advertise actual provider availability and resource limits; multiple flows share admission to exclusive backend workers.

Autoconverters negotiate the representation needed at a connection: local native frames/PCM, encoded packets, artifacts or segments. Prefer compatible direct transport and hardware codecs where supported. Make inserted conversion steps inspectable. A continuous stream needs an explicit capture interval or segmentation policy before becoming an artifact.

Reuse shared encoder sessions when source and output profiles match. Slow consumers get bounded independent queues. Current Ogg encoding is software and two-pass; fragmented-file readability and live codecs vary by platform. Verify capabilities rather than treating a common API as universal support.

A cache/record writer registers a **growing asset** before completion:

- Stable asset/write identity, one writer and independent readers.
- Complete timestamped audio chunks/video fragments, with readable and durable ranges.
- Playback of available data while the write continues, including from another flow.
- Sealing to an immutable revision without invalidating readers.
- Explicit behavior for seek limits, cancellation, failure and storage exhaustion.

Keep mutable write sessions separate from immutable content hashes. AI jobs consume explicit snapshots/ranges. Current Stage has growing local-file playback; early database publication and audio/video extent access are new work.

### Controls, code, decisions and inspection

Generated widgets, graph controls, automation, MIDI/OSC and spatial/timeline views address stable shared parameters/events. Define arbitration between writers, readback without echo, and persistent binding identities. MIDI/OSC can wire directly into a graph or learn onto a UI widget's underlying target. Preserve existing APC ownership, lighting blackout/arming and Behringer transport guards.

**Splash code** appears as a typed code card: inputs, outputs, a short description and runtime status. Open it to edit source and inspect bounded input/output snapshots. An enum output feeds a normal route/switch with labeled wires. Complex logic stays code; do not force every expression into graph boxes or promise arbitrary bidirectional code/graph conversion. Start from Flow's existing Fn implementation, with explicit state nodes and invocation budgets.

**AI decisions** use the same typed routing interfaces. Jev supports choice, score and yes-probability questions; its current input is text/structured state, so media first passes through analysis or transcription. Confidence is not a correctness guarantee. Use provider-neutral nodes, explicit timeout/freshness/fallback policies, bounded retries and recorded decisions for replay. Credentials remain local secret references. [TypeSafe primitives](https://docs.typesafe.ai/primitives), [state](https://docs.typesafe.ai/concepts/state), [confidence](https://docs.typesafe.ai/confidence).

**Animated wires and probes** display actual activity: values/events, audio meters/waveforms, video previews, asset progress and code/AI decisions. Reuse the canvas pulse machinery. Probes observe existing work without activating branches, rerendering producers or introducing hidden monitor audio. Bound sampling, retained payloads and preview cost; shed probe work before affecting audio.

### Application capabilities

| Capability | Required graph expression |
|---|---|
| Synth/DJ/audio wiring | Independent instruments, decks, effects, inserts/sends, mixers, cue/program outputs and clocked transport. Current fixed strip counts become presets. |
| Audio Units | Instruments/effects and working native editors through libs/external_audio. Device I/O stays in platform. VST remains a stretch backend. |
| VJ and spatial composition | Arbitrary layers with effects/transforms/crop/order; nested scenes export frames. The spatial editor modifies graph values, not a parallel scene model. |
| Karaoke | Timed lyric track on the transport clock, rendered as a video layer; microphones as normal audio inputs; optional vocal removal; song queue as a playback bin. |
| Presentations | Slides, media and live sources as scenes with cued transitions; presenter view (notes, next, timer) and audience output are separate surfaces of one flow; remote/clicker control through normal bindings. |
| Live production (OBS replacement) | Screen/window/camera/browser capture as sources; scenes, overlays and transitions from the VJ components; audio mixing with per-source sync offsets; simultaneous recording, streaming and virtual camera outputs. |
| Web/browser input | One owned session exposes separate audio/video ports; preserve LIVE/CACHE and audio-routing behavior. |
| Lighting and external mixer | Graph components for fixture/scene controls, Art-Net/DMX output and typed Behringer control/state. |
| Hosted Makepad apps | Reuse the --stdin-loop shared-frame protocol and child lifecycle as a video source. Audio transport is an explicit additional capability. |
| Recording and editing | Record sources/program; select source ranges and arrange them in a seekable edit flow immediately usable by other graphs. No export required for reuse. |
| YouTube live | A real encode/mux/RTMPS output with independent lifecycle and health; local playback/recording continue during network trouble. |

Audio Unit parameter enumeration, automation and observers are partly new work. The plugin owns actual values; the graph mirrors them and schedules writes. Native-editor changes update the mirror without echo. Restore opaque state before explicit saved overrides. Closing an editor does not stop its plugin; destruction waits for both UI and audio retirement.

Timeline edits reference stable source revisions or explicit growing-source snapshots. Seeking does not rerun AI jobs or reconstruct a live application's past; record those sources when history is needed.

YouTube requires live audio packets, muxing and ingest beyond the existing file encoder. Reuse native facilities and supported hardware codecs. Validate actual remote delivery, not merely local encoding. [YouTube encoder settings](https://support.google.com/youtube/answer/2853702?hl=en).

### Custom interfaces and controllers (direction, not specification)

This describes where the UI layer is heading so the engine can prepare for it. Nothing here is fixed; the engine is built first and these ideas must not constrain it. Read it as a list of things the engine should not make hard.

**Intended experience.** A user describes the inputs and outputs they want ("a YouTube video mixer") and AI builds the graph. The user tweaks it, then asks for an interface. They can draw over the graph to select which parts belong together; each selection becomes a dockable panel, much as one would explain a DJ deck to a designer. AI proposes the most likely design from domain knowledge (decks, queues, mixers, waveforms) and can follow references such as "more like Traktor or VirtualDJ". The user refines in words or by hand: sliders instead of knobs, dropdowns instead of radio buttons. Nobody starts from scratch, and shipped templates are starting points that remain fully editable.

**Responsive containers.** Related widgets (for example transport buttons) live in a flex/grid container. Containers can be moved within a panel and drag-scaled from a corner to reflow as a row, column or square. Panels dock, tab and float.

**State-dependent widgets.** As on MIDI controllers, one widget can serve several functions depending on state: a knob selects what a slider or button targets, or a mute button acts on whichever channel's tab is visible. Hardware and on-screen controls share this model, so AI can map any MIDI controller onto (parts of) a graph, and a user-built panel is simply another controller.

**What the engine should anticipate:**

- Stable addresses and rich metadata on every parameter/event: type, range, unit, step, enum labels, default, display hint and semantic role (transport, gain, crossfade, cue). AI and templates choose widgets from this, not from node internals.
- Grouping as data: a graph selection or subflow boundary can be named and referenced by a panel, and survives graph edits.
- Binding indirection: a widget binds to a target expression that may resolve through a selector or state value, with prepared retargeting, soft takeover/pickup and feedback to motorised or LED hardware. This extends the arbitration and echo-free readback already required above.
- UI state (visible tab, focused deck, shift/layer, page) exposed as ordinary graph values so bindings and code can depend on it.
- One widget description for screen and hardware: layout, skin and physical mapping are separate layers over the same bindings, so swapping knob for slider or remapping a controller never touches the graph.
- Cheap, rate-limited observation for many live widgets (meters, waveforms, thumbnails), shed before audio like probes.
- Panels and controller mappings saved as separate reusable Splash modules inside the app package, editable by AI through the same scoped, revision-checked operations.

### AI iteration and sharing

Expose compact component interfaces and scoped graph/UI edit operations to AI. Reuse catalogued modules first. Edits check the expected revision, validate dependencies, prepare changes and preserve the previous runnable revision on failure.

The app package contains identity, entry flows, generated UI, pinned modules/assets, defaults, plugin requirements and capability roles. Recipient-specific devices, endpoints and credentials bind locally. Do not bundle live handles or installed plugin binaries.

Define this manifest and revision contract early. Finish export/import UX after the runtime is proven. Stage initially opens the bundle; compatible embedded hosts can use the same definitions. Any future standalone wrapper uses the same format.

## Refactor boundaries

| Donor | Extraction |
|---|---|
| libs/flow | Extend registry, module loader, codec, instances and scheduling. Reuse job executors; add provider contracts rather than copying the engine into Stage. |
| libs/flowgraph and apps/flow-ui | Keep the canvas generic. Extract reusable editor/controller/faces, potentially into libs/flow_ui; hosts retain app menus, service startup and networking policy. |
| apps/stage/engine and Stage DSP/media code | Retain the existing shared engine. Separate algorithms and services from App/widgets; replace fixed indices at node boundaries with instance/port identities. |
| Stage pipelines/gen and asset libraries | Migrate Stage generation directly to Flow, then remove its asset-creator dependency. Extend asset data/store/client contracts for growing writes. |
| AI hub/livepipe and platform codecs | Reuse job/session protocols and encoders; adapt shared-lock, unbounded-queue and blocking teardown boundaries before integration. |
| platform Audio Unit code | Move plugin hosting to libs/external_audio, restore native UI integration and add parameter/state contracts. Do not move hardware audio I/O wholesale. |
| show_control, apps/mixer and Stage APC/MIDI code | Reuse protocol, mapping and device behavior with bounded commands and asynchronous lifecycle. |
| WM/Director hosted-app support | Extract shared transport, frame ownership and child lifecycle without importing either app shell. |

No opaque “legacy Stage” node counts as migration. Complete each feature when graph instances actually own and wire it. Keep the legacy console usable until equivalent presets work, and preserve shared consumers such as Flow and the karaoke engine.

## Delivery and validation rules

The separate Stage repository has local branch **flowgraph** at the recorded baseline; its active main checkout and dirty donor work were preserved. Use a bounded sibling worktree for implementation and integrate donor changes explicitly. Shared Makepad changes follow its existing local → work → dev rules. [Recorded baseline](../agent_state/stage-flowgraph/source-baseline.json).

Codex manages scope/integration and reviews Fable's work. Reuse the persistent Fable session for design and implementation; Grok can take bounded mechanical/validation tasks. This document is the active plan. [Fable's review](../agent_state/stage-flowgraph/fable-final-review.md) and [disposition](../agent_state/stage-flowgraph/review-disposition.md) retain review evidence; the longer draft is historical reference only.

For each step:

- Check affected packages/platforms, format changed Rust and run relevant existing tests. Do not add generated test scaffolding without an explicit request.
- Use release builds and the owned native-GPU remote workflow for runtime evidence. Exercise the stated completion criterion and record exact revision/platform/provider coverage.
- Keep audio-continuity, main-thread work and queue/drop measurements for media changes. Preserve existing consumers and report baseline failures separately.
- Missing hardware, codecs, providers or toolchains remain missing coverage. Do not install software implicitly or claim a substitute proves the requested path.

These are completion gates, not a request to implement during this planning task.

## Step-by-step implementation plan

Each step should land as small, reviewable changes. Complete its dependency and verification before promoting it; do not treat a whole step as one large patch.

### 1. Establish the implementation baseline

Prepare the Stage flowgraph worktree, reconcile the recorded dirty donor changes and inventory current feature/platform/test coverage. Identify the existing engine and application consumers that must keep working.

**Done when:** the starting revision and donor ledger are explicit, owning workspaces build as expected, and pre-existing failures are recorded.

### 2. Establish the shared engine/provider contract

Extend the shared registry and embedding API. Define stable identities, typed ports/controls, bins, execution ownership, capability discovery and bounded commands/observations. Define the app manifest/revision contract now. Keep concrete devices and media resources behind providers.

**Done when:** Stage and a minimal second host can construct the engine without a Stage shell or mandatory canvas/server/device runtime.

### 3. Implement modular Splash loading and safe saving

Add component resolution, pinned references, nested instances, named imports/exports and ordered variadic connections. Extend the reader and writer together. Preserve source-owned modules and diagnose unsupported serialization or recursive definitions.

**Done when:** load → save → load preserves existing recipes and a multi-file nested graph, including code bodies and stable connections.

### 4. Extract the reusable editor and code views

Move shared graph projection/editing, faces and bindings out of the Flow app shell. Add component navigation, source/instance context, named-connection reveal and typed code cards with source editing.

**Done when:** Flow and Stage use the shared facilities; graph edits preserve module boundaries and source-owned code.

### 5. Implement instance lifecycle and activation

Add finite admission limits, demand-aware branches, shared-producer accounting and per-instance persistent control VMs. Separate parameter writes, route selection and prepared topology revisions. Track cancellation, result delivery and resource retirement by generation.

**Done when:** two flows run independently; only a selected generation branch executes; shared fanout executes once; changing editor focus or replacing one instance does not restart another.

### 6. Prove embedding in Sandbox

Migrate the bounded model.concepts image-generation path to direct Flow execution. Preserve seeds, gallery progress, cancellation, selection and prepared previews; connect selected output to the asset service. Leave unrelated creator consumers intact.

**Done when:** Sandbox runs the reusable definition through its own UI/services and Stage loads the same definition without an engine fork.

### 7. Deliver the first mixed Stage graph

Wire webcam → triggered frame capture → existing image-to-video generation → prepared clip → simple layer/scene → program, with independent asset publication. Instantiate the scene inside an outer flow.

**Done when:** a real generated clip enters the running mix, the old program continues during work/failure, and the hierarchy saves/reloads. Missing camera/provider access remains explicit missing coverage.

### 8. Extract realtime audio and clocks

Turn decks, synths, native effects, channel strips, routing and cue/program outputs into independently wireable nodes. Compile active flow partitions into one device schedule. Extract capture, BPM analysis, clock/source selection and loop/quantized transport components.

**Done when:** synth/deck → effects → mixer works without fixed topology; named clocks drive loops; unrelated flow edits preserve state; audio remains continuous under video/background load.

### 9. Restore external Audio Unit hosting

Create libs/external_audio from platform plugin-hosting code. Add safe render/control/native-view ownership, format negotiation, stable parameters/observers, state restore and prepared replacement. Register instrument/effect nodes.

**Done when:** MIDI/sequencer → AU instrument → AU effect → mixer runs with working native editors, synchronized controls, save/reload and correct teardown. Use already available plugins; VST remains stretch scope.

### 10. Generalize VJ composition and spatial editing

Extract source/player, effect, transform, transition and composition primitives. Replace the fixed two-texture topology with an ordered multi-layer composition. Add spatial editing over the same graph values.

**Done when:** at least three processed layers work, a scene is reused inside another scene, and transform/crop/order edits survive undo and save/reload. All GPU work stays main-thread; overload drops video without affecting audio.

### 11. Integrate GenAI routes and media conversion

Inventory and expose all existing finite and realtime backend capabilities. Add inspectable format conversion and shared encoder-session ownership. Adapt live-session transport, admission and source/result handling to bounded operation.

**Done when:** VJ/video → live GenAI → mix and captured media → slower generation → playback/assets work on available backends. Codec/backend gaps are explicit, and slow generation never stalls output.

### 12. Migrate browser input and growing asset bins

Expose browser audio/video separately. Wire cache/record output into an early database-visible growing asset, including audio chunks, video fragments, independent readers and sealing. Preserve LIVE/CACHE and single-session ownership.

**Done when:** one flow writes while separate audio/video flows read before completion and continue through sealing. Writer failure is reported and does not block audio or the UI.

### 13. Integrate controllers, lighting and the Behringer mixer

Extract reusable MIDI/APC40 and OSC mappings, lighting components and guarded mixer control/state. Replace incompatible locking/queue/teardown seams. Connect direct value/event routing and device feedback.

**Done when:** controller input drives graph values and physical outputs with preserved reservations/guards and no feedback loops; closing a surface does not stop owned output services.

### 14. Complete shared bindings and live switching

Bind ordinary sliders, dials, dropdowns and buttons to graph values/events. Add MIDI/OSC learn to those same targets, explicit writer arbitration and prepared route/module switching.

Keep bindings indirect enough for later state-dependent targets (see [Custom interfaces and controllers](#custom-interfaces-and-controllers-direction-not-specification)); parameter metadata must be sufficient to choose a widget without inspecting the node.

**Done when:** direct wires, learned controls and UI readback agree; dials select/crossfade routes; compatible modules switch without interrupting audio or retargeting unrelated bindings.

### 15. Add runtime AI decision components

Add provider-neutral choice/score/predicate interfaces and a Jev adapter. Combine decisions with Splash policy code and visible route branches. Implement input snapshots, deadlines, stale-result rejection, fallback and bounded retry/replay.

**Done when:** an AI decision chooses a prepared route or generation subflow, only required work runs, and delayed/failed decisions leave a declared working path active.

### 16. Add wire activity and probes

Connect runtime observations to wire animations and type-specific inspectors. Support named connections and nested instances; bound sampling, history and previews.

**Done when:** probes show actual values/audio/video/assets/decisions without extra producer execution. Inspection overload sheds observations and leaves audio untouched.

### 17. Add AI-generated application surfaces

Expose compact catalog/interface inspection and scoped graph/UI editing. Generate reusable Splash surfaces bound to component interfaces, with revision checks, validation, preview and rollback.

First pass at the custom-interface direction: graph selections become dockable panels of responsive containers, AI proposes widgets and controller mappings from parameter metadata and domain references, and the user adjusts by instruction or direct manipulation. Scope is decided when this step starts; templates follow.

**Done when:** AI can revise a workflow and its controls without flattening modules, losing bindings or replacing the last runnable revision with a partial edit.

### 18. Extract hosted Makepad app inputs

Reuse WM/Director transport, shared frames, pacing, input/resize and child lifecycle behind a reusable service. Keep GPU operations main-thread and treat app audio as an explicit transport capability.

**Done when:** an owned release app using --stdin-loop supplies reusable video to a graph and closes cleanly without affecting unrelated app instances.

### 19. Implement recording and reusable edit flows

Record source tracks and program output through bins. Add range selection and arrangement components with stable clip identities, explicit timebases and worker-based seeking. Build the timeline UI over those components.

**Done when:** a recorded webcast/demo can be trimmed and arranged, then played immediately inside another graph while recording continues, with correct A/V timing and no required export.

### 20. Implement YouTube live output

Complete the needed live audio packet, muxing and RTMPS publishing path using native codec/network facilities. Add destination lifecycle, reconnect/health and bounded output queues; share compatible encoders with recording.

**Done when:** a configured test destination receives playable output while local recording/playback continues. Network trouble does not block audio; actual remote delivery is verified.

### 21. Package the workflow app and complete migration

Finish export/import of the manifest, pinned module closure, UI, asset references and capability/plugin requirements. Support local role binding and clear missing-dependency diagnostics. Complete graph presets for current Stage modes, remove replaced Stage orchestration and its asset-creator dependency, and verify affected shared consumers.

**Done when:** a separate local profile opens the app, binds available resources, runs it, edits its hierarchy and rolls revisions back. No secrets/live handles are exported, and no required behavior remains owned only by the legacy UI.

