# Omagari VFX system: specification and roadmap

This document is the work order for the agents that extend Omagari and integrate it into the game. It describes **what** to build, **why**, the **file format**, the **runtime API**, the **editor changes**, and **acceptance criteria** per phase. Items marked *verify* are best guesses about Bevy 0.19 / bevy_hanabi 0.19 APIs that the implementing agent must confirm in the crate sources (`~/.cargo/registry/src/*/bevy_*-0.19.1`, `bevy_hanabi-0.19.0`).

Owners used below:
- **Omagari agent**: works in this repository (`C:\bevy\omagari\omagari`): library (runtime plugin + data model), editor, MCP server, docs.
- **Game agent**: works in the game repository; consumes the library. Game phases start only after the user confirms.

---

## 0. Scope and principles

- **Omagari is a separate, reusable project.** No game-specific code or names live in Omagari. The game depends on the `omagari` library and maps its own rules (statuses, XP, attacks) to Omagari parameters and triggers.
- **Scope is driven by the game's needs**, plus **broad bevy_hanabi coverage** (everything except 2D and multi-camera), because which hanabi features the effects will need is not known in advance.
- **Every feature is available three ways: editor UI, project file, MCP.** A person without an agent must be able to do everything by hand; the agent must be able to do everything through MCP. Acceptance criteria of every feature include its UI.
- **The editor previews through the same runtime as the game.** What the editor shows is what the game shows.
- Names used in this document (`poison`, `sword`, `intensity`, `charge`, `hit`, …) are **examples**. The system has no built-in parameter, trigger or effect names.

### Glossary
- **VFX package**: one authored effect: layers + parameters + triggers + assets. One file.
- **Layer**: one visual part of a package: particles, surface, shape, trail, afterimage, light, distortion.
- **Instance**: one playing copy of a package attached to one target entity. A target can have several instances at once.
- **Parameter**: a named value declared by the package author (any name, any number), set by the game at runtime per instance, smoothed by the plugin, usable by any layer value.
- **Binding**: a layer value that is a constant or a curve over a parameter.
- **Trigger**: a named one-shot event (any name, any number) fired by the game, optionally with a position and direction.
- **Socket**: a named point on the target: a node/bone name in the target's hierarchy plus an optional local offset, or just an offset from the target root.
- **Target**: the entity the instance is attached to (a character root, a weapon entity, a prop).

---

## 1. Goals

Every visual effect in the game (status effects, weapon effects, enemy auras, props, hits) is **authored and previewed in Omagari, saved as one file, and played in the game with a few lines of code**.

A package can contain:

| Capability | Example |
|---|---|
| Particle layers (bevy_hanabi, nearly all 3D features, see F9) | mist, bubbles, drips, sparks, additive glow, flipbooks, ribbons, mesh particles |
| Surface layers (shader drawn over a model's meshes, with optional volume shells) | purple scrolling noise + glowing rim on a poisoned body; glowing energy skin with thickness |
| Shape layers (simple 3D shapes with the same shader) | shield bubble, glowing column, ground ring, shockwave, stylized slash arc |
| Trail layers (ribbon between two moving points) | sword swing trail |
| Afterimage layers (fading ghost copies of a model) | sword "shadows" left along a swing |
| Light layers | weapon glow lighting the floor |
| Distortion layers | heat haze |
| Parameters (free names and count; number or colour) | `intensity` 0..1 set from game progress; `tint` colour per enemy |
| Triggers (free names and count, optional position/direction) | `tick` on damage, `hit` at the contact point, `swing_start` / `swing_end` |
| Attachment | whole target, socket (bone/node + offset), segment between two sockets |
| Size fit | effect authored for a 1.8 m body fits a 4 m enemy |
| Lifecycle | start, keep running while refreshed, stop smoothly (fade out), auto-cleanup |

Hard requirements from the user:
1. Everything is **visible, importable and editable in Omagari**, by hand and through MCP. What the editor shows must be what the game shows.
2. Omagari **imports textures** now (required) through a generic asset library that later takes more asset kinds (models, meshes, animations).
3. The runtime is a **Bevy plugin** usable on any entity (player, enemies, props, weapons).
4. Effects react to **game rules** only through parameters and triggers; the plugin knows nothing about the rules.

### Non-goals
- No gameplay logic in the plugin.
- Omagari never changes the game's models or their materials. Effects are drawn **on top of / around** models.
- No node-graph shader editor: one well-parameterised effect shader (+ distortion shader).
- No texture generator and no channel-packing tool (possible future work: a "pack 4 grayscale images into RGBA" tool).
- No real volumetric rendering (raymarched fog). Volume comes from shells, shapes and particles.

---

## 2. Current state (facts, read from the code)

Updated after P0a (data model split), P0b (format v2 + runtime slice; editor and MCP on v2), P1 (instances complete: sockets, segments, fit, restart, auto-finish, hot reload, preview motion), P1b part 1 (F10 editor UX base: layout, live preview, undo/redo, dirty state, shortcuts, tooltips, inline validation, camera presets, new from recipe), P1b part 2 (F3 asset library: ASSETS tab, import by dialog / drag & drop / MCP, channel view, extract channel, texture checks, dev screenshot flags) P2 (F2 parameters and triggers: runtime, expressions, bindings, editor tabs, MCP, guide) P2b (F9 hanabi coverage: every registered 3D modifier, all operators and value types, effect settings, spawner presets, coverage test) and P3 (F5 model, animation and socket preview: CAPSULE / MODEL stages, glTF models and clips from other files, hierarchy browser, socket gizmos + drag, weapon preview, segment-with-radius preset, animation triggers, `describe_model`, checks against the model) and P4 part 1 (F4 surface layers: `OmagariEffectMaterial` with its own skinning/morph vertex shader, overlays per shell over the target's own meshes, editor inspector, MCP checks / descriptions / recipes, WGSL validation test).

### Data model (`src/format/`, library, serde only)
- **v1** `OmagariProject { effects: Vec<EffectEditor> }` (`format/mod.rs`), `EffectEditor` + v1 `produce()` (`format/particles.rs`), modifiers (`format/modifiers.rs`), expressions (`format/expr.rs`). Only migration and the golden tests use v1 now; the egui code lives in `src/editor/`. v1 `produce()` keeps the hard-coded texture slot `"color"`, `AlphaMode::Blend`, `ParticleTexture(ModulateOpacityFromR)`, `Orient(AlongVelocity)` and silently drops wrong-list modifiers (legacy path, golden-tested).
- **v2** `ProjectV2` and its defs (`format/project.rs`): parameters (`ParamDef`, kind = variant of `default: Float(..) | Color(..)`), triggers + pulses, assets (`AssetDef { id, kind: Texture(TextureAssetDef) }`, purpose/wrap/color_space/filter/grid, `None` = purpose default), `fit: Option<FitDef>`, `layers: Vec<LayerDef>` (only `Particles(ParticlesLayerDef)` so far), sockets, `preview: Option<PreviewDef>`; `Binding<T> = Const | Param(name, keys)` with `evaluate`. Every struct is `#[serde(default)]`; unknown struct fields are ignored (no warning yet). Asset helpers on `ProjectV2`: `asset_users`, `rename_asset` (updates layers), `delete_asset` (refused while used), `unique_asset_id`. `TextureSlotDef { asset, channel: ChannelDef (R|G|B|A|Luminance, default Luminance) }` is the texture slot type of Omagari's own shaders (P4 on); not used by the runtime yet. Everything in the format is used by the runtime (P2): parameters, triggers + pulses, `rate`, `visible`, `play` (Loop, OnStart, OnTrigger, While), attach, sockets, fit. Helpers: `ProjectV2::{param, trigger, param_users, trigger_users, rename_param, rename_trigger, unique_param_name, unique_trigger_name}` (renames update expressions, `rate`, `visible`, `play`, pulses and the preview block), `ParamValue::{kind, to_value}` + `From<f32 | Vec4 | LinearRgba | Color>` (colours are linear RGBA), `ParamDef::clamp`, `VisibleRangeDef::factor` (1 inside `[from, to]`, linear fade to 0 over `fade` outside), `PlayDef::{trigger, is_driven}`, `Binding::param`.
- Expressions: `ExprWriterEditor::Property(name)` reads a runtime property (`RUNTIME_PROPERTIES` in `format/expr.rs`: `segment_a`, `segment_b`, `trigger_pos`, `trigger_dir` (vec3), `trigger_strength`, `visibility` (f32); `runtime_property_default`: `trigger_dir` = +Y, strength / visibility = 1); `ExprWriterEditor::Param(name)` reads a package parameter: `EffectWriter::with_params` declares it once as property `param_property_name(name)` (`param_` + an injective WGSL-safe encoding of the free-text name) with the parameter's default (f32 / vec4). Operators are table-driven: `OPERATORS: [OperatorInfo { name, args, hint, make }]` + `ExprOperatorEditor::{name, info, args, args_mut, produce}`; P2 added `Mix(a, b, t)`, `Clamp(x, low, high)`, `SmoothStep(low, high, x)`, `Min`, `Max` (hanabi `mix` / `clamp` / `smoothstep` / `min` / `max`). `ParticlesLayerDef::produce(params, warnings)` (OnTrigger / While layers produce with `starts_active = false`), `params_used`; `ParticleEffectDef::produce(name, space, params, warnings)`, `properties_read`, `params_read`, `exprs[_mut]`, `emit_duration`; `ExprWriterEditor::visit[_mut]`, `ModifierEditor::exprs[_mut]`.
- `ParticleEffectDef { capacity, spawner: SpawnerDef, alpha_mode, init/update/render modifiers }` + `produce(name, simulation_space, warnings)` (`format/particles.rs`): texture sampling and orientation are render modifiers (`ParticleTexture`, `Orient`), blending is `alpha_mode`; the texture slot is added only with a `ParticleTexture` modifier. Wrong-list modifiers are skipped with a warning (checked with hanabi's `Modifier::context()`).
- P2b (F9): modifiers `SetPositionCone3d` (init only: its code reads the spawner transform that only hanabi's init shader has, so `ModifierEditor::contexts()` overrides hanabi's Init|Update), `RadialAccel`, `TangentAccel`, `KillSphere` (`radius_expr`, produced as radius²), `KillAabb`; render `SetColor` / `SetSize` (hanabi 0.19 takes `CpuValue` constants there, not expressions: `CpuVecDef` Single/Uniform; a Uniform gives every particle the same value because the render shader has no per-particle seed), `Flipbook { grid: Option<(rows, cols)> }` (None = the texture asset's grid; produced as hanabi's `(columns, rows)`; `ParticlesLayerDef::produce_in(project)` / `ProjectV2::texture_grid` pass it, `sprite_index_over_lifetime(frames)` is the prebuilt update `SetAttribute`), `Round { roundness_expr }`, `ScreenSpaceSize`, `Orient.rotation_expr: Option<Expr>` (skipped when `None`). `ModifierEditor::{contexts, all_defaults}`, `RenderModifierEditor::{kind_name, exprs[_mut], all_defaults, produce_boxed}`; `ParticleEffectDef::exprs[_mut]` include render expressions. Settings `motion_integration`, `simulation_condition`, `prng_seed` (written only when not default, so old files and goldens are unchanged); `SpawnerDef::{with_preset, preset}` (`SpawnerPreset::{Once, Rate, Burst}` = hanabi's constructors, count and burst period kept). Expressions: `OPERATORS` (59 rows, `group` for the menu) covers hanabi's unary, binary and ternary operators plus `Vec4`, `Attr`, `ParentAttr`, `Cast(expr, ValueTypeDef)`; `EXPR_VALUES` covers literals of every scalar/vector type (f32, i32, u32, bool, vec2..4, ivec, uvec, bvec), `Rand(type)` and the built-ins (`DeltaTime`, `Virtual*`, `Real*`, `IsAlive`, `AlphaCutoff`). `format/coverage.rs`: exhaustive matches over hanabi's operator enums and small enums, `SKIPPED_MODIFIERS` (empty), `SKIPPED_FEATURES` with reasons. hanabi 0.19 limits found: no integer/bool random function in its shaders (`RandU32` / `Rand(i32..)` fail on the GPU → check error), `AlphaCutoff` writes `sim_params.alpha_cutoff` (does not exist → check error), `Uniform`/`Normal` need operands of a type known at shader-writing time (literals, attributes, `Cast`), `Asin`/`Acos` of an out-of-range constant fail to compile, `SizeOverLifetime.screen_space_size` is ignored, gradients are linear only, `EmitSpawnEventModifier` is missing from hanabi's `register_modifiers()`.
- P3 preview block (`PreviewDef`, editor only, new fields written only when not default so old files and goldens are unchanged): `stage` gains working `Capsule` / `Model`; `model`, `animation` (clip name or `Animation<index>`), `animation_speed`, `attach_to` (existed) plus `root_height` (target above the model's feet, m), `animation_file` (clip from another glTF with the same node names; `animation_source()`), `animation_triggers: [(trigger, clip time)]`, `target: Body | Weapon` (`PreviewTargetModel`: which model `attach_to` names and the instance is on), `weapon: Option<PreviewWeaponDef { model, node, offset (m, ignoring the node's scale), rotation (degrees XYZ, `quat()`), scale }>`; `shows_model()`. `rename_trigger` also renames animation triggers. Prebuilt init modifier `segment_point_with_radius(radius)` (`format/modifiers.rs`): `position = Mix(segment_a, segment_b, rand) + normalize(cross(b − a, rand vec) + tiny) × radius × √rand` (editor Position preset "Random point on segment with radius"; the tiny bias keeps a zero-length segment finite).
- `format/hanabi_serde.rs`: the file format of the hanabi values in the model is Omagari's (attribute names, `SpawnerDef`/`CpuValueDef`, `ShapeDimension`, `EventEmitCondition`, `ColorBlendMode/Mask`), byte-identical to hanabi 0.19's serde. In memory the model still holds hanabi's `Attribute` etc.
- `ProjectV2::from_ron_str[_with_warnings]` reads v1 (no `version` + `effects`) or v2; `migrate_v1` (`format/migrate.rs`) per 4.5 (built-in textures become `omagari://textures/<file>` assets with ids `cloud`, `glow1`, ...; v1-dropped modifiers are dropped with a warning). `to_package()` clears `preview`.
- Tests: `tests/format_golden.rs` (v1 goldens; migrated projects export byte-identical to them; v2 goldens in `tests/golden/v2/`, incl. a hand-written `inputs/showcase.omagari.ron`).

- P4 part 1: `LayerDef::Surface(SurfaceLayerDef { name, targets: AllMeshes | Meshes([..]) | Materials([..]), material: EffectMaterialDef, shells: [ShellDef { offset (m), opacity, noise_offset }], fade: FadeDef { in, out } (serde `in` / `out`), visible })` (`format/surface.rs`). `EffectMaterialDef { blend: Blend | Add, color: Binding<Vec4>, opacity, emissive, noise_a / noise_b: Option<NoiseDef { texture: TextureSlotDef, mapping: Triplanar | Uv, scale, scroll: Binding<Vec2>, strength, contrast }>, rim: Option<RimDef { color, power, intensity }>, pulse: Option<EffectPulseDef { frequency, amount }>, flash: Option<Binding<f32>>, dissolve: Option<DissolveDef { threshold, edge_width, edge_color }>, fill: Option<FillDef { axis: AxisDef, from, to, softness }>, mask: Option<MaskDef { texture, mapping, scale, strength }> }`; `evaluate(lookup) -> EffectValues` (unknown parameters = the curve's first key), `binding_params`, `rename_param`, `texture_slots[_mut]`, `is_color_binding`; `evaluate_color`: a colour binding may read a Color parameter directly (`Param("tint", [])`). `MeshTargetsDef::matches(entity name, glTF mesh name, glTF material name)` (material falls back to the part of `<mesh>.<material>` after the last dot). `FadeDef::step` (linear level, restart fades back up). `LayerDef::{name_mut, kind_name, as_surface[_mut]}`, `ProjectV2::surface_layers`; `asset_users` / `rename_asset` / `param_users` / `rename_param` include surface layers. Built-in `omagari://textures/noise.png` (tileable fBm, 256², `BUILTIN_NOISE_TEXTURE`, purpose Noise via `builtin_texture_purpose`; appended to `BUILTIN_TEXTURES`, not a v1 particle texture). Goldens unchanged.

### Runtime (`src/runtime/`, added by `OmagariPlugin`)
- Built-in textures (`runtime/builtin.rs`): the PNGs in `assets/` are `include_bytes!`-embedded into Bevy's `embedded` source as `embedded://omagari/textures/<file>`; `resolve_asset_path` maps `omagari://textures/<file>` to it (a real `omagari` source would need registering before `AssetPlugin`).
- `VfxPackage` + `VfxPackageLoader` (`runtime/package.rs`, extensions `vfx.ron`, `omagari.ron`): migrates, loads textures as dependencies (loader settings from the texture def when not the sprite default: `texture_image_settings`; Bevy keeps a path's first loader settings, so `apply_texture_settings(&mut Image, def)` fixes an already loaded image — the editor preview uses it, a game's hot reload of changed texture settings does not yet), one `EffectAsset` per particle layer as labeled sub-asset `Layer<i>`, logs warnings (unsupported attach/play, missing texture/parent, wrong-list modifiers). `VfxPackage::build(project, &mut impl PackageAssets, warnings)` also builds from an in-memory project (`(&mut Assets<EffectAsset>, &AssetServer)`).
- Parameters (`runtime/params.rs`): `VfxInstance.params: ParamState` = per declared parameter a target and a smoothed value (`smooth`: `current += (target − current)(1 − e^(−dt/smoothing))`, colours per channel, Float clamped) plus an optional pulse (jump to `to`, linear back over `decay`); made from the package (`sync`, values kept by name + kind, clamped to new ranges) on first use, on `replace_package` and on hot reload (`resync`); values set before load are pending. `update_params` (sync + tick) → `upload_layer_values`: `EffectProperties::set_if_changed` of each layer's `param_properties`, the `visible` factor (→ `VfxLayerState.visibility` + property `visibility`) and the `rate` binding (`evaluate_binding`, shared helper for later CPU-bound values → `VfxLayerState.rate`). `VfxWarnOnce` resource: one warning per (package, name) for unknown names / wrong kinds / unknown triggers.
- Triggers (`runtime/triggers.rs`): `VfxInstance.triggers` queue (waits while Starting, dropped while Stopping) → `apply_triggers`: pulses, and `VfxLayerState.burst = Some(TriggerArgs)` on `OnTrigger(name)` layers → `drive_layer_spawners` (replaces `stop_layer_spawners`; also stops stopping/retired layers): writes `trigger_pos` / `trigger_dir` / `trigger_strength` in the layer's simulation space (`TriggerArgs::in_layer_space`; no payload = origin, +Y, 1), then `EffectSpawner::reset()` + `active` (once the spawner exists); `While(param, t)` sets `active = value ≥ t`, `reset()` on the rising edge. `restart` skips OnTrigger / While layers. `scale_spawn_counts` (PostUpdate, after `EffectSystems::TickSpawners`, visible layers only): with a `rate` binding `spawn_count = rate × visibility × dt` (fraction carried; only while the spawner is active and not completed), without one hanabi's count × visibility. Chose this over `SpawnerSettings::set_count`, which hanabi samples only at the start of a cycle (≤ 1 s latency for a rate spawner).
- Instances (`runtime/instance.rs`): `commands.spawn_vfx(&pkg, target)` / `spawn_vfx_with(&pkg, target, VfxSpawn { params })` → `VfxInstance` entity `ChildOf(target)`; `spawn_vfx_at[_with](&pkg, Transform)` → instance in the world (`target` = itself, no fit). `VfxControl::{set_param, set_param_now, param, trigger, trigger_at}`; layer entities carry `VfxLayerState`. Layers (`VfxLayer { instance, index }`) spawn when the package and every layer texture have settled (also for in-memory packages). `VfxControl::{stop, restart, replace_package, is_alive, state}`; stop deactivates `EffectSpawner`s, despawns after the stop duration of the build (max init lifetime incl. parent chain, 5 s if unknown). `restart` while Stopping → Running on the same layers (spawners `reset()` + active; no-op while Running). `VfxPackage::auto_finish` = last spawn + linger when every root layer is `OnStart` with finite cycles → the instance finishes by itself. `VfxFinished` is sent once whenever an instance goes away (finished, target despawned, despawned by hand; observer `On<Remove, VfxInstance>`). Hot reload: `AssetEvent::Modified` → running instances rebuild layers in place (old layers "retired": spawners off, despawned after their linger), parameter values kept, and the package's textures get its settings (`apply_texture_settings_to`, so changed sampler / colour-space settings reach already loaded images); `replace_package` uses the same path; a layer despawned with its socket node triggers a rebuild.
- Attach (`runtime/attach.rs`): `Socket` layers are reparented under the resolved node (package socket node, else a node with that `Name`, breadth-first, VFX entities skipped), local transform = offset × k, scale k = instance world scale / node world scale; pending/unresolved → under the instance at the offset, warning after `SOCKET_TIMEOUT` (3 s); re-resolved on any `Changed<Children>`/`Changed<Name>`. `Segment` layers stay under the instance; `write_segment_properties` (PostUpdate after transform propagation) writes `segment_a`/`segment_b` (layer-local for Local, emitter-relative world axes for Global) into `EffectProperties` when the effect declares them. Fit (`apply_fit`): instance local scale = desired world scale / target world scale; `TargetHeight` measures the world AABB height of `TargetHierarchy::own_meshes` (Aabb component, else `Mesh::final_aabb`, else positions). **Hanabi `Local` applies the emitter's full transform (incl. scale) to particle positions and quad size (`vfx_render.wgsl` `transform_position_simulation_to_world`); `Global` only adds the emitter translation at spawn (`SimulationSpace::eval`), so fit scales Local layers only (loader + check warn for Global).** Helpers (P3): `world_bounds` (world AABB of local boxes; `world_height` uses it), `socket_offset_for` (inverse of `socket_world_position`, for dragging), `mesh_bounds` is public (skinned glTF meshes have `DynamicSkinnedMeshBounds`, so their `Aabb` follows the animation). Own-model rule: stops at nested model roots (`WorldAssetRoot` — Bevy 0.19's renamed `SceneRoot` —, `DynamicWorldRoot`, `ScenePatchInstance`) below the target's own model and at `VfxSeparateModel`. Tests: `tests/runtime_instance.rs`, `tests/runtime_attach.rs`, `tests/runtime_params.rs` (MinimalPlugins + TransformPlugin; spawners and visibility stood in for).

- Effect material (P4, `runtime/effect_material.rs`): `OmagariEffectMaterial { uniforms: EffectUniforms (16 × vec4, 256 bytes), noise_a, noise_b, mask: Option<Handle<Image>> (bindings 1–6, Bevy's white fallback when empty; sampler/colour space from the image = the texture asset's settings), alpha_mode (Blend / Add), sort_bias }`; `Material`: own `vertex_shader` + `fragment_shader` = embedded `embedded://omagari/shaders/omagari_effect.wgsl` (inserted into `EmbeddedAssetRegistry` like the built-in textures), prepass and shadows off, `depth_bias` = sort bias only (Bevy 0.19 uses a material's depth bias for the transparent sort, not as a pipeline depth bias). `MaterialPlugin` is added when `PbrPlugin` is present, else only the asset type (headless tests). `EffectUniforms::new(values, ShellInputs, textures)` packs everything (a part without its texture is off). Constants `DEPTH_PUSH` (2 mm toward the camera along the view ray, × fit: no z-fighting at offset 0, same pixels), `SHELL_SORT_STEP` (1 mm of sort distance per shell: inner shells draw first). Shader (`src/shaders/omagari_effect.wgsl`): vertex stage mirrors bevy_pbr 0.19.1 `mesh.wgsl` (`vertex`, `morph_vertex`) with `bevy_pbr::{skinning, morph, mesh_functions, forward_io::Vertex, view_transformations}`, same `#ifdef SKINNED` / `MORPH_TARGETS` structure; outputs world position / normal, UV, the **rest-pose (pre-skinning, pre-morph) position in pattern units** (rest position × metres per rest unit / fit: skinned meshes take it from the CPU (one joint's skinning matrix), rigid meshes from their own transform) and rest normal, and the fill coordinate; shells push along the skinned world normal by `offset × fit`. Fragment: noise A/B (UV or triplanar in the rest pose, channel pick, scroll by `globals.time`, contrast), colour × emissive, rim (× noise), flash, pulse, dissolve (discard + edge), fill band, mask, fade; Add outputs premultiplied `(rgb·a, 0)` under `BLEND_PREMULTIPLIED_ALPHA`; `tone_mapping` under `TONEMAP_IN_SHADER`. All textures are sampled before any discard and without branches (uniform control flow).
- Surface layers (P4, `runtime/surface.rs`): per surface layer an entity `VfxSurfaceLayer { instance, index }` + `SurfaceLayerState { level, visibility, materials (one per shell), overlays, rest_bounds, rest_scale }`, child of the instance (`VfxInstance::surface_layers()`); spawned with the particle layers in `start_instances` (a rebuilt layer of the same name keeps its fade level; old surface layers are despawned at once). `sync_surface_overlays` (after `upload_layer_values`): one material per shell; when the hierarchy or a mesh/skin changed, `surface_meshes` = `TargetHierarchy::own_meshes` filtered by `targets` (names from `Name`, `GltfMeshName`, `GltfMaterialName`); per mesh and shell an overlay `VfxSurfaceOverlay { layer, source, shell }` as a **child of the mesh** at the identity transform with the same `Mesh3d`, `SkinnedMesh` (same joints), `MeshMorphWeights`, `DynamicSkinnedMeshBounds` / `NoFrustumCulling` as the source, `NotShadowCaster` / `NotShadowReceiver`; overlays of meshes that are gone or no longer match are despawned; rest bounds = union of the covered meshes' asset bounds (fill), rest scale from the first skin. `update_surface_materials`: fade level (`FadeDef::step`, running vs stopping), `visible` factor, uniforms written only when they change, overlays hidden while the level × visibility is 0. Observer `on_surface_layer_removed` despawns the overlays (they are not the layer's children). `PreparedSurfaceLayer { layer_index, def, textures: [noise_a, noise_b, mask] }` in `VfxPackage::surface_layers` (textures loaded like particle textures, settled before the layers spawn, hot-reload texture settings applied); `stop_duration` includes `fade.out`; a package with surface layers never auto-finishes. `TargetHierarchy` skips overlays (own meshes, socket search).

### Legacy export `*.hanabi.ron` (`src/lib.rs`)
- `ExportedProject` / `ExportedEffect` serialized through **Bevy reflection** (hand-written (de)serializers in `lib.rs`); every modifier must be reflection-registered in the game (`register_missing_hanabi_modifiers`). No longer written (editor EXPORT and MCP write `*.vfx.ron`); kept loadable and golden-tested.
- `EffectComplex::spawn(commands, textures, pos, despawn_in)`: fixed world position, returns the spawned entities, textures passed in `PARTICLE_TEXTURES` order; the timer stops the spawners and despawns each effect after its particle life (read from the produced `SetAttribute(lifetime)`, `max_effect_lifetime`; incl. parent chain). Test: `tests/legacy_effect.rs`.

### Editor scene and UI (`src/editor/`, `main.rs` only wires plugins)
- `EditorPlugin` (`editor/mod.rs`) adds the systems. Scene (`editor/scene.rs`): `Stage` = Air / Floor / Object (rock) / Capsule / Model (P3; the room's floor at `StageFloor` below the target), synced from `preview.stage`; `Backdrop` = light / dark / black (editor setting).
- Cameras (`editor/camera.rs`): `PreviewCamera::Orbit` (`PanOrbitCamera`, real time) and `PreviewCamera::Game` (fixed offset from the moving `PreviewTarget`), one active per `preview.camera`. Each uses a render profile of `omagari_settings.ron` (`RenderSettings { orbit, game }` in `settings.rs`: projection Perspective(fov°) / Orthographic(height m), yaw°, pitch°, distance m, tonemapping, bloom preset + intensity, exposure EV100, background; partial profiles fill in their own preset's defaults). Game default: orthographic `ScalingMode::FixedVertical { 16 }`, yaw 45°, pitch 35.26°, offset (20, 20, 20), `Hdr`, `Bloom::NATURAL`, TonyMcMapface. Settings window edits them live; F = focus the orbit camera on the target.
- Layout: top toolbar (`toolbar.rs`: name + unsaved marker, NEW (empty / guide recipe via `mcp::guide_recipes`), OPEN, SAVE, SAVE AS, EXPORT, undo/redo, STAGE, CAMERA, BG, SETTINGS), left LAYERS (`layers_panel.rs`: select, add, duplicate, delete with confirm, reorder, per-layer 👁 hide / S solo for the preview only (`layer_view.rs`: hidden layers get `AlphaMode::Mask(2.0)` in the preview build, so they still simulate and spawn children), problem badges), right tabs (`inspector.rs`: LAYER inspector (`effect.rs` widgets in a grid), SOCKETS & FIT (`package.rs`), PREVIEW (stage, camera, backdrop, motion), PARAMETERS / TRIGGERS (`params_panel.rs`, see below), ASSETS (`assets.rs`, see below)), bottom bar (`playback.rs`: RENDER, STOP, RESTART, Live toggle, pause, speed = `Time<Virtual>` relative speed, problems counter, build warnings, status). Modals in `dialogs.rs` (unsaved changes: Save / Don't save / Cancel; delete layer; save as). Tooltips in `hints.rs`.
- Commands are `Action`s (`actions.rs`) queued by buttons, shortcuts (`shortcuts.rs`: Ctrl+S/E/Z/Y/Shift+Z/D, Delete, F, Space; only Ctrl+S/E while a text field has focus), the window close button (`close_when_requested: false`) and MCP preview requests; `perform_actions` asks before discarding unsaved changes (New, Open, MCP preview, Quit) or deleting a layer.
- Session (`session.rs`, `history.rs`): `track_project` serializes the project each frame it changed; `History<String>` = snapshot undo/redo (200 steps), a drag commits on release, typing after 0.8 s idle; dirty = current text is not the saved revision (window title `● name - Omagari`). Live preview: when the package part (all but `preview`) changed and stayed for 150 ms, `PreviewCommand::Render` → `replace_package`. `check_project` reruns 300 ms after an edit; `Diagnostics::about(subject)` maps messages to layers / sockets / assets.
- Asset library (`src/asset_library.rs`, shared with MCP; `image` crate, editor feature): `import_file` (outside the game folder → copy to `vfx/textures/`, name collision → `_2`, identical bytes → reuse; inside → relative path), `guess_purpose` (file name words), `add_texture_asset` (same path → same asset), `extract_channel[_asset]` (8-bit grayscale `<stem>_<channel>.png` next to the file, built-ins → `vfx/textures/`; asset copied with the new path), `texture_info[_at]` (header: size, channels, bits), `scan_textures`, `decode_preview`. ASSETS tab (`editor/assets.rs`): list with thumbnails (decoded on `AsyncComputeTaskPool`, RGBA8 Bevy `Image` → `EguiUserTextures`, re-decoded when the file's mtime changes), details with RGB/R/G/B/A view, settings (purpose, wrap/colour space/filter as "default from purpose" or explicit, grid), users, rename / delete / extract / add built-in as `Action`s. IMPORT = `rfd::AsyncFileDialog` blocked on a background thread (no parent window), polled each frame; `FileDragAndDrop` → `Action::ImportFiles` (+ drop overlay). `PreviewState::textures` + `sync_preview_texture_settings` apply edited settings to loaded images. `bevy/jpeg` is enabled by the `editor` feature only.
- Parameters and triggers (P2): PARAMETERS tab (add, rename through `rename_param` with a typed draft committed on Enter / focus loss, kind, default (colour picker + HDR numbers), range, smoothing, users, problems per `parameter "name"` subject; live slider / colour picker writing `preview.params`; "animate" = `EditorSession::param_animation`, min → max → min over 4 s, not saved), TRIGGERS tab (add, rename, pulses, ⚡ FIRE / "at random point" = `Action::FireTrigger` → `PreviewCommand::Fire` → `PreviewState::pending_fires` (waits up to 30 frames for a just-spawned instance), auto-fire every N s = `preview.auto_triggers`, "auto-fire at random points" session toggle). `controller::drive_preview` sends the values with `set_param` (`set_param_now` on a new instance), fires pending and auto triggers. Layer inspector: Play combo with OnTrigger (trigger picker) / While (parameter + threshold), Rate (`binding.rs::binding_editor`: constant / curve over a Float parameter, key list, small plot; reusable for later layer values), Visible range. Expression UI: `Param` (picker with kind, unknown warning), operator menu from `OPERATORS` (named arguments for 3-argument operators), prebuilt "Mix By Parameter".
- Dev flags (`editor/dev.rs`): `--open`, `--camera`, `--tab`, `--asset`, `--channel`, `--import`, `--screenshot <png>` + `--screenshot-after <frames>` (Bevy `Screenshot` observer, then `AppExit`), `--window-size`.
- Preview runs through the runtime (`src/controller.rs`): `apply_preview_commands` builds a `VfxPackage` from `LayerView::apply(project)` and `replace_package`s the live instance (or `spawn_vfx` on the `PreviewTarget`); STOP = `VfxControl::stop`; RESTART despawns the instances and spawns a new one. `move_preview_target` moves the target along `preview.motion`.
- Runtime fix (`runtime/render_fix.rs`): bevy_hanabi 0.19 leaves despawned `Opaque`/`Mask` effects in Bevy's retained binned phases and panics drawing them; a render system removes non-mesh phase items whose draw entity is gone.
- Settings (`src/settings.rs`): `omagari_settings.ron` (`game_assets`, `render`), `--game-assets`, `OMAGARI_GAME_ASSETS` (CLI > env > file), game assets fixed at startup; asset source `game` registered before `DefaultPlugins` (the repo's `assets/` when unset). Paths resolve through `runtime::resolve_asset_path_in(path, source)` (editor: `Some("game")`, games: default source).
- Model stage (P3, `editor/model.rs`): `ModelPreview` resource. `sync_preview_models` spawns the body under `PreviewTarget` (capsule `PreviewBody` mesh, or `WorldAssetRoot(GltfAssetLabel::Scene(info.scene))` from the `game` source, `root_height` below the target, turned π around Y so glTF +Z faces the direction of travel), the weapon (`PreviewWeapon`, `WorldAssetRoot`, child of its node found by name under the body, `weapon_transform` cancels the node's world scale) and sets `StageFloor` (floor depth: capsule 0.9, model `root_height`); `resolve_instance_target` picks the instance's entity (root, weapon, or the `attach_to` node of the chosen model; the root while it is missing) and restarts the preview instance when it or the body changes (fit and sockets start over); `drive_model_animation` loads the clip by index from the animation file's `ModelInfo` (`GltfAssetLabel::Animation(i)`), `AnimationGraph::from_clip` → `AnimationGraphHandle` on the model's `AnimationPlayer` (Bevy adds one to models with animations; a model without any gets `AnimationTargetId::from_names(path)` + `AnimatedBy` on its named nodes and a player on each scene root node, `add_animation_targets`), applies speed / pause / loop (`RepeatAnimation`) / seek (`set_seek_time`), reads time and duration, and pushes `animation_triggers` into `PreviewState::pending_fires` when the clip time passes them (`passes`, loop-aware). Verified on Mixamo files: clips of other files play without retargeting because Bevy's glTF loader targets nodes by the name path from the scene root node (`Armature/mixamorig:Hips/...`), identical across Mixamo exports. `update_socket_markers` (PostUpdate after transform propagation): world bounds of the instance target's own meshes (`target_bounds`, used by "at random point": `random_point_on_bounds`), socket markers (package sockets + raw node names used by layers, world + screen position, pending flag) and segment ends, drawn as `Gizmos<SocketGizmos>` (depth bias −1: on top of the model); labels painted by egui over the viewport. `drag_sockets`: a press within 10 px of a package socket's dot drags it in the camera plane (`InfinitePlane3d` through the dot, `Ray3d::intersect_plane`), the offset is set from `socket_offset_for` (mm rounding); the orbit camera is disabled while a dot is hovered or dragged and the drag is one undo step (`Activity::Dragging`).
- Model UI (`editor/model_panel.rs`): PREVIEW tab model section (model file picker from `scan_models`, root height + "0.9 (player root)", animation file + clip pickers with a skeleton-mismatch warning, speed, instance on body / weapon, attach-to node picker, weapon file / node / offset / rotation / scale, problems, model summary); 🌲 HIERARCHY tab (body or weapon, search, tree with kind, per node 🎯 attach_to, ⚓+ socket (`new_socket_for`, name after the `prefix:`), A / B segment ends of the selected layer, mesh entities and materials); bottom bar animation controls (pause, loop, scrub slider, speed); TRIGGERS tab "fire at clip time". SOCKETS & FIT: node picker from the target model, ◎ select, nudge buttons (1 mm / 1 cm / 10 cm), world position, "show in viewport", measured target height. Stages `CAPSULE` / `MODEL` in the toolbar and PREVIEW tab. Dev flags `--stage`, `--model`, `--animation`, `--animation-file`, `--attach-to`, `--root-height`, `--anim-time`, `--motion`, `--focus`, `--backdrop`, `--tab hierarchy`.
- `src/model_info.rs` (editor feature; `gltf` crate with the features Bevy's loader already compiles, so nothing new is built): `ModelInfo::load` reads only a model's JSON (a `.glb`'s first chunk) — nodes in scene order with Bevy's names (`GltfNode<i>` when unnamed), parents, bones (skin joints), mesh nodes, skinned flag, rest transforms; meshes with primitive entity names (`<mesh>.<material>`, Bevy's `primitive_name`) and materials; animations with duration (input accessor `max`) and target node paths; rest-pose bounds (skinned meshes in bind-pose model space, rigid ones placed by their node). `cached` (path + size + mtime), `node`, `path`, `is_under`, `rest_position`, `animation` (name or `Animation<i>`), `matching_targets` (clip vs model by name path), `kind`, `has_mesh_or_material` (P4), `summary`, `describe`; `scan_models`.
- Surface layers in the editor (P4, `editor/surface.rs`): ➕ ADD → 🎨 Surface layer (`new_surface_layer`: additive purple, built-in noise added to the assets, rim); inspector with name, covers (all / meshes / materials, picker from the preview target model's meshes, mesh entities and materials, a typed name without a model), shells (rows, + shell, presets "skin" / "thick glow"), fade, visible, blend, colour, opacity, emissive, optional parts with a checkbox (noise A/B with texture + channel picker, mapping, scale, scroll, strength, contrast; rim; pulse; flash; dissolve; fill with axis; mask); every value through `binding_editor_with` (constant / curve over a Float parameter / — colours — a Color parameter; generic over f32, Vec2, Vec4; `binding_editor_range` with min/max). Hidden / solo: a hidden surface layer loses its shells in the preview build. Tooltips in `hints.rs`. The preview's texture-settings sync covers surface texture slots. Dev flags added: `--orbit yaw,pitch,distance`, `--focus-node <name>`, `--screenshot-crop x,y,w,h`, `--stop-at <frame>`, `--fire <trigger>`.

### MCP server (`src/mcp/`)
- `mod.rs`: JSON-RPC over stdio, tool list and dispatch; `guide_recipes()` parses the guide's recipes (tests, editor NEW from recipe); `preview_in_editor(name, stage, camera, motion, params, trigger)` writes `saved_effects/.preview_request` as JSON `{name, stage, camera, motion, params: {name: number | [r,g,b,a]}, trigger}` (`PreviewRequest`; names and kinds validated against the project; params become `preview.params`, the trigger is fired once the project plays; the editor still reads the old two-line form). `analyze_project(params: [{..}, ..])` → `analyze_project_with_sets`. Accepts v1 or v2, writes v2 (`write_project` keeps v2 text as written, migrates v1), `read_project` shows v1 files migrated, `export_project` writes `.vfx.ron` (game assets `vfx/` or `exported_effects/`).
- Asset tools: `import_asset(path, purpose?, id?, project?)`, `list_assets`, `extract_channel(project, asset, channel)` (project tools refuse `examples/`); `preview_in_editor` also takes `camera`.
- P3: `describe_model(path, max_nodes?)` (relative to the game assets folder or absolute) → `ModelInfo::describe`; `list_assets` adds the models with their summary; `preview_in_editor` takes stages `capsule` / `model` and `model`, `animation`, `animation_file`, `attach_to`, `root_height` (checked against the files: model exists, clip in its file, `attach_to` a node; `PreviewRequest::apply_to`; a model sets the MODEL stage). `check_project` → `check_preview_model`: missing model / weapon / animation files are warnings (the editor shows the capsule), unknown clip, `attach_to` and weapon node are errors, clip nodes missing from the model and animation triggers past the clip's end are warnings, and the node names below the instance target (the target model, below `attach_to`, plus a held weapon) turn socket nodes and non-package attach names into errors when missing (NOTES only without a model). Animation triggers are checked like auto triggers. Guide: section "model", attach section on `describe_model` and the checks, recipe "Blade sparks with a radius" (capsule preview, segment preset, swing burst).
- P2b in `analysis.rs`: the evaluator handles every operator and value type (`Val` with int/uint/bool vectors, WGSL semantics: integer division by 0 = dividend, `Round` ties to even, `Sign(0)` = 0), with shader contexts (`IsAlive` update only, `ParentAttr` init only); update modifiers RadialAccel, TangentAccel, KillSphere, KillAabb and setters in the update list are simulated, as is `motion_integration` (Pre/Post/None); render features are described (`render_text`: flipbook grid, rounded corners, screen-pixel size, rotation, settings; colour and size through SetColor / SetSize / ScreenSpaceSize). Checks: wrong lists from `contexts()` (SetAttribute and the position/velocity setters are now allowed in update), render expression types, flipbook grid / sampling / `sprite_index`, rotation with AlongVelocity, render-time random values, SetColor/SetSize ranges, ScreenSpaceSize below 2 px, cone `Surface`, motion integration None with velocities.
- `analysis.rs`: `check_project(&ProjectV2, assets_root)` (assets by id, built-in names, missing texture files, unused assets, texture paths (absolute = error, `..` / `\` = warning), unknown extensions, JPG note, purpose vs settings (Noise+Clamp, Gradient+Repeat, Noise/Mask+Srgb), grid (0 = error, not dividing the image / non-Sprite = warning), 16-bit grayscale PNG (loads as an integer texture), render modifiers, sockets (names, duplicates, unused), attach (non-package socket names are NOTES — `Diagnostics::notes`), segment property use, unknown properties, child vs parent attach, fit (reference > 0, Global layers not scaled), parameters and triggers (`check_params_and_triggers`, `check_layer_bindings`: names, duplicates, property-name clashes, ranges, smoothing, unknown / Color params in expressions, `rate`, `visible`, `While`, pulses (also on Color params), unknown OnTrigger triggers, unsorted / empty curves, trigger payload / visibility reads without OnTrigger / visible, preview values and auto triggers; unused params / triggers = notes, subjects `parameter "x"` / `trigger "x"`), play vs spawner (also OnTrigger with a looping spawner, rate with a finite one, OnTrigger / While / rate on child layers), Global + looping + lifetime ≥ 1 s trail warning at 3.6 m/s or the preview motion speed, package round trip), CPU simulation `simulate_project` (on `resolve_for_analysis(project, values)`: segment ends at the sockets' offsets, trigger payload = origin / +Y / 1, `Param` → literals, `rate` × `visible` → spawner count, While below threshold → 0, OnTrigger = fired once at t = 0; the evaluator knows `Mix`, `Clamp`, `SmoothStep`, `Min`, `Max`), `analyze_project` (fit, render settings, simulation space, attach, play / rate / visible / parameter reads per layer; "Parameters and triggers" and "Parameter sets" sections: default sets = every Float parameter at min / middle / max, per layer count, size, mid-life colour and reach per set), tests at the end (guide recipes are v2 and must pass with zero warnings and notes).
- `examples/*.omagari.ron` are v2 (the migrated goldens); their v1 originals are golden inputs in `tests/golden/inputs/`.
- P4 (`mcp/surface_check.rs`): `check_surface_layers` (targets against the preview target model via `ModelInfo::has_mesh_or_material` — errors, NOTES without a model —, empty target / shell lists, shell offsets < 0 or > 0.1 m and opacity outside 0..1, fades < 0, texture slots (missing asset = error, empty slot with strength > 0 = warning), dissolve without noise, rim power ≤ 0, fill from > to, parameter reads (unknown, Color where a Float is needed, curves without keys / unsorted, keys on a Color parameter), visible range, a `Blend` overlay covering with opacity ≥ 0.95 at the defaults or a parameter's min / max (`cover_opacity`: colour alpha × opacity × innermost shell × noise at its average), layer names unique across kinds); `describe_surface_layer` (targets, blend, colour × emissive, opacity, coverage, noise text with scroll speed on the body, rim width, pulse, flash source with the pulsing triggers, dissolve, fill, mask, shells and glow thickness, fades, visible, every parameter at min / max); `surface_sets_report` in "Parameter sets". `check_project` accepts projects without particle layers; unused assets use `asset_users`. Guide: section `surfaces`, recipes "Status skin", "Energy shell", "Frost from the feet up" (capsule preview, zero warnings), texture table + noise, design feedback rows.
- `docs/CLAUDE_VFX_GUIDE.md` is compiled in (`include_str!`, `mod.rs`); keep it in sync with `analysis.rs`.

---

## 3. Target architecture

### 3.1 One data model, one runtime, three front ends
```
                 ┌──────────────────── omagari (library crate) ─────────────────────┐
                 │ format/   data model (serde only, no egui): ProjectV2, layers,     │
                 │           particle effect model (former EffectEditor/modifiers/   │
                 │           expr), produce() -> hanabi EffectAsset, migration       │
 game ──uses──▶  │ runtime/  OmagariPlugin: VfxPackage asset + loader, instances,    │ ◀──uses── editor (bin, feature "editor")
                 │           params, triggers, particles, surface, shape, trail,     │            ├─ egui UI (draw_ui impls live here)
                 │           afterimage, light, distortion                           │            ├─ asset library + import
                 │ shaders/  embedded WGSL                                           │            ├─ model/animation preview
                 │ legacy    EffectComplex (*.hanabi.ron), kept loadable             │            └─ MCP server (--mcp)
                 └───────────────────────────────────────────────────────────────────┘
```
- **The game loads the authoring data directly.** The package file has the same schema as the project file; the runtime calls `produce()` at load time to build hanabi `EffectAsset`s. No hanabi reflection serialization, no per-modifier registration in the game, migrations are ours, hot reload works.
- The editor previews by spawning instances through the runtime API.

### 3.2 Module layout
```
src/lib.rs                  // OmagariPlugin, re-exports, legacy EffectComplex
src/format/
  mod.rs                    // ProjectV2, LayerDef, ParamDef, TriggerDef, Binding, AssetDef, AttachDef
  particles.rs              // particle effect model (moved from effect.rs/modifiers.rs/expr.rs) + produce()
  migrate.rs                // v1 OmagariProject -> ProjectV2
src/runtime/
  mod.rs                    // runtime plugin parts, system sets
  package.rs                // VfxPackage asset + loader (loads textures as dependencies), hot reload
  instance.rs               // instance components, spawn API, lifecycle
  attach.rs                 // socket/segment resolution, size fit
  params.rs                 // smoothing, binding evaluation, hanabi property upload
  triggers.rs               // trigger events, one-shot layers, pulses, payload
  particles.rs              // particle layer -> hanabi entities
  surface.rs                // overlay meshes + OmagariEffectMaterial
  shape.rs                  // shape layer
  trail.rs                  // trail layer (CPU ribbon mesh)
  afterimage.rs             // afterimage layer
  light.rs                  // light layer
  distortion.rs             // distortion layer
src/shaders/omagari_effect.wgsl       // vertex (rest-pose position, inflate) + fragment
src/shaders/omagari_distortion.wgsl
src/editor/                           // editor-only: UI, panels, undo, import, preview stage
src/mcp/                              // unchanged location
```
Rules:
- The library never depends on egui or editor-only crates (`editor` feature gating as today).
- UI code for a data type lives in `src/editor/` (e.g. `impl DrawUi for SetPositionSphereDef`), never in `src/format/`.
- Shaders embedded with `embedded_asset!` (*verify* 0.19 path) so the game adds only the plugin.
- MCP mode: nothing but JSON-RPC on stdout.

---

## 4. Data model and files

### 4.1 Files
| File | Written by | Read by | Contents |
|---|---|---|---|
| `saved_effects/<name>.omagari.ron` | editor, MCP | editor, MCP | Project v2: package data + `preview` block (editor-only) |
| `<game assets>/vfx/<name>.vfx.ron` | editor EXPORT, MCP `export_project` | game runtime, editor | same schema without `preview`, `format_version` set |
| `*.hanabi.ron` | legacy | legacy loader | kept loadable, no longer written |

`ProjectV2` and the package share one Rust type; export = clear `preview`, write. The runtime ignores `preview` if present, so the game can also load a `.omagari.ron` directly during development.

### 4.2 Asset references and the game assets folder
- Assets are referenced **by path relative to the game's `assets/` folder** (`"vfx/textures/noise_perlin.png"`). Built-ins use `"omagari://textures/cloud.png"` (embedded in the library).
- Editor setting **Game assets folder** (persisted in `omagari_settings.ron` in the repo root; also `--game-assets <dir>` and `OMAGARI_GAME_ASSETS`). The editor registers an asset source `game` for it (*verify* `register_asset_source` 0.19 API); the game loads the same paths from its default source.
- Without a configured game folder, the editor uses `assets/` of this repo as the game folder (so the editor still works for other users).

### 4.3 Asset library (F3)
The project has an `assets` list. Each entry: `id` (name used by layers), `kind`, `path`, kind-specific settings. Kinds now: `Texture`. Later: `Model`, `Mesh`, `Animation` (added as new kinds, same panel, same import flow).

Texture settings:
- `purpose`: `Sprite` | `Noise` | `Mask` | `Gradient`. Chooses defaults for the settings below.
- `wrap`: `Repeat` | `Clamp` (noise defaults to Repeat).
- `color_space`: `Srgb` (colour) | `Linear` (data: noise, masks). Purpose picks the default.
- `filter`: `Linear` | `Nearest`.
- `grid`: `Option<(rows, cols)>` for sprite sheets (flipbooks).

Using textures:
- Every texture slot in Omagari's own shaders (surface, shape, trail, afterimage, distortion) is `(asset: id, channel: R|G|B|A|Luminance)`. One-layer internet textures use `Luminance`; channel-packed textures can be used once per channel.
- Particle layers use hanabi's texture sampling (`ImageSampleMapping`: `Modulate`, `ModulateRGB`, `ModulateOpacityFromR`, *verify*), which cannot pick G/B/A. Import therefore offers **"Extract channel to a separate texture"**, writing a grayscale PNG.

Import:
- Editor: **Import** button (native file dialog, `rfd` crate, editor feature only) and **drag & drop** onto the window (Bevy `FileDragAndDrop`). MCP: `import_asset(path, purpose?)`.
- Files outside the game folder are **copied** into `<game assets>/vfx/textures/` (name collision → suffix). Files inside are referenced in place.
- Formats: PNG, JPG; KTX2 if the Bevy feature is enabled.
- **Assets panel**: thumbnails, per-channel view (R/G/B/A buttons), settings, rename, delete (blocked while used, shows users), warnings for missing files.

### 4.4 Project v2 sketch (`*.omagari.ron`)
```ron
(
    version: 2,
    parameters: [
        // any names, any number. kind: Float | Color. smoothing: seconds to ~63% of a new target.
        (name: "intensity", kind: Float, default: 0.0, min: 0.0, max: 1.0, smoothing: 0.4),
        (name: "tint", kind: Color, default: (0.65, 0.08, 0.95, 1.0), smoothing: 0.2),
        (name: "flash", kind: Float, default: 0.0, min: 0.0, max: 1.0, smoothing: 0.0),
    ],
    triggers: [
        (name: "tick", pulses: [(param: "flash", to: 1.0, decay: 0.35)]),
        (name: "hit"),                              // may carry position/direction from the game
    ],
    assets: [
        (id: "mist",  kind: Texture((path: "omagari://textures/cloud2.png", purpose: Sprite))),
        (id: "noise", kind: Texture((path: "vfx/textures/noise_perlin.png", purpose: Noise))),
    ],
    fit: None,                                      // None | TargetScale | TargetHeight(1.8) (F1)
    layers: [
        Particles((
            name: "mist",
            attach: Target,                         // Target | Socket("name") | Segment(from: "a", to: "b")
            simulation_space: Local,                // Local | Global
            play: Loop,                             // Loop | OnStart | OnTrigger("hit") | While(param, threshold)
            rate: Param("intensity", [(0.0, 10.0), (1.0, 30.0)]),
            visible: None,                          // Some((param: "intensity", from: 0.6, to: 1.0, fade: 0.1))
            effect: ( /* particle effect model: capacity, spawner, texture, init/update/render modifiers */ ),
        )),
        Surface((
            name: "skin",
            targets: AllMeshes,                     // AllMeshes | Meshes([..]) | Materials([..]); own model only
            material: ( /* EffectMaterialDef, see F4 */ ),
            shells: [(offset: 0.0), (offset: 0.02, opacity: 0.5)],   // volume
            fade: (in: 0.2, out: 0.4),
        )),
        Shape(( name: "shield", attach: Target, shape: Sphere(radius: 1.0), material: ( /* … */ ), fade: (in: 0.1, out: 0.3) )),
        Trail(( name: "slash", from: "blade_base", to: "blade_tip", length: 0.2, emit: Trigger(start: "swing_start", stop: "swing_end"), material: ( /* … */ ) )),
        Afterimage(( name: "ghosts", interval: 0.04, lifetime: 0.25, emit: Trigger(start: "swing_start", stop: "swing_end"), material: ( /* … */ ) )),
        Light(( name: "glow", attach: Socket("blade_mid"), color: Param("tint", ..), intensity: Param("intensity", [(0.3, 0.0), (1.0, 8000.0)]), range: Const(3.0), flicker: 0.1 )),
        Distortion(( name: "haze", attach: Segment(from: "blade_base", to: "blade_tip"), shape: Capsule(radius: 0.12), strength: Param("intensity", [(0.8, 0.0), (1.0, 0.015)]), noise: (asset: "noise", channel: Luminance) )),
    ],
    sockets: [                                      // package-level named points (used when the model has no such node)
        (name: "blade_base", node: None, offset: (0.0, 0.05, 0.0)),
        (name: "blade_tip",  node: None, offset: (0.0, 0.95, 0.0)),
    ],
    preview: (                                      // editor-only
        stage: Model,                               // Air | Floor | Object | Capsule | Model
        camera: Game,                               // Orbit | Game (render profile from settings)
        model: Some("models/ybot.glb"),
        animation: Some("Run"), animation_speed: 1.0,
        attach_to: None,
        params: [("intensity", Float(0.5))],
        motion: Some(Circle(radius: 3.0, speed: 3.6)),
        auto_triggers: [("tick", 1.0)],             // fire every N seconds
    ),
)
```
Notes:
- `Binding<T>` = `Const(T)` | `Param(name, keys: [(param_value, T)])`, piecewise linear. `T` is `f32`, colour or vec3. A `Color` parameter can be bound directly (`Param("tint", Identity)` *exact syntax to define*).
- Every field has `#[serde(default)]`; v1 files and partial files load.
- Unknown fields are tolerated with a log warning (newer editor, older game).

### 4.5 v1 → v2 migration
- `version` missing ⇒ v1. Each `EffectEditor` becomes `Particles(...)` with `attach: Target`, `simulation_space: Global` (keeps the old look), `play: Loop` or `OnStart` (from `cycle_count`), `texture_index` → a built-in texture asset entry. The formerly hard-coded render settings (texture sampling, orient along velocity, alpha blend) become explicit render modifiers / settings with the same values.
- Test: every file in `examples/`, `saved_effects/` and every guide recipe loads, migrates, produces the same `EffectAsset` as before (compare produced modifiers), and round-trips v2.

---

## 5. Feature specifications

Each feature: design, runtime API, editor UI, MCP, tests, AC. "AC" = acceptance criteria.

### F1. Instances: attach, follow, local space, fit, smooth stop, hot reload

**Design**
- `VfxPackage` asset + loader (`.vfx.ron` and `.omagari.ron`). Loads referenced textures as dependencies. Builds hanabi `EffectAsset`s once per (package, particle layer) and stores their handles; instances share them (per-instance differences come from properties, F2).
- An instance is an entity, child of the target (`ChildOf(target)`), with `VfxInstance { package, state, params, … }`. Layer entities are children of the instance.
- Attach (`attach.rs`): `Target`; `Socket(name)`: package socket → descendant node with `Name == node` (+ offset), or offset from target root if `node` is `None`; `Segment(from, to)`: two sockets resolved each frame, positions written into hanabi properties `segment_a`/`segment_b` (layer local space). Resolution retries every frame until the glTF scene exists, then caches; re-resolves when the hierarchy changes.
- **Own model only**: a surface layer covers meshes under the target but stops at descendants that are another model root (entities with `SceneRoot`, or marked `VfxSeparateModel`). A weapon parented to a hand bone is therefore not covered by a body effect. To cover both, the game spawns the package on both.
- **Size fit** (`fit`): `None`; `TargetScale` (inherit target scale); `TargetHeight(reference)`: measure the target's mesh AABB height once loaded, scale the instance by `height / reference`. *Verify* hanabi Local space applies the emitter's scale; if not, upload `scale` as a property and multiply sizes/positions/velocities in produce().
- `simulation_space` per particle layer (`EffectAsset::with_simulation_space`, `bevy_hanabi-0.19.0/src/asset.rs:443`).
- Lifecycle: `Starting → Running → Stopping → Finished`. Stop: `EffectSpawner.active = false` on particle layers (`spawn.rs:694`), fade the other layers over `fade.out`, despawn after `max(particle lifetime, fades, trail length)`, send `VfxFinished { instance, target }`. `restart()` while Stopping → Running.
- **Hot reload**: when a package asset changes (Bevy file watcher), running instances rebuild their layers and keep their parameter values. Lets the user tune in the editor and see it in the running game.
- Legacy: `EffectComplex` keeps working; `spawn` returns `Vec<Entity>`; abrupt despawn replaced by graceful stop.

**Runtime API**
```rust
app.add_plugins((HanabiPlugin, OmagariPlugin));

let pkg: Handle<VfxPackage> = asset_server.load("vfx/poison.vfx.ron");
let inst: Entity = commands.spawn_vfx(&pkg, target);
let inst = commands.spawn_vfx_with(&pkg, target, VfxSpawn { params: &[("intensity", 0.3.into())], ..default() });
let inst = commands.spawn_vfx_at(&pkg, Transform::from_xyz(..));        // one-shot in the world, auto-cleanup

fn system(mut vfx: VfxControl) {
    vfx.set_param(inst, "intensity", 0.8);          // smoothed
    vfx.set_param_now(inst, "tint", Color::srgb(0.2, 0.9, 1.0));
    vfx.trigger(inst, "tick");
    vfx.trigger_at(inst, "hit", TriggerArgs { position: Some(p), direction: Some(d), strength: 1.0 });
    vfx.stop(inst);  vfx.restart(inst);  vfx.is_alive(inst);
}
// message VfxFinished { instance, target }
```
Unknown parameter/trigger names: warn once per (package, name), never panic.

**Editor**: preview uses this API. Preview options: motion (circle / back-and-forth at a set speed), auto triggers.

**MCP**: `analyze_project` reports simulation space and warns for `Global` + looping + long lifetime (trail length estimate = speed × lifetime; speed from `preview.motion` or 3.6 m/s).

**Tests** (MinimalPlugins, no render): spawn → stop → `VfxFinished` once after the expected time; restart during Stopping; target despawn cleans up; socket resolution on a hand-built hierarchy; own-model-only mesh collection; fit scaling math.

**AC**: a looping Local package on a moving entity stays around it with no trail; `stop` fades, nothing pops, `VfxFinished` once; several instances on one target are independent; editing the package file updates the running instance; old `.hanabi.ron` files still load.

### F2. Parameters and triggers

**Parameters**
- Declared per package: name (free), kind (`Float` | `Color`), default, range (Float), smoothing. Any number per package. The plugin has no built-in names.
- Per instance: `current`, `target`. Each frame `current += (target - current) * (1 - exp(-dt / smoothing))`; `smoothing = 0` = immediate; clamp Float to `[min, max]`. Colors smooth per channel.
- **Particle expressions** (`format/particles.rs` expression model):
  - `Param(name)` → hanabi property (`ExprWriter::add_property` / `prop`, `bevy_hanabi-0.19.0/src/graph/expr.rs:2438,2505`), declared once per effect asset for every parameter it uses. Float → f32 property, Color → vec4.
  - New operators `Mix`, `Clamp`, `SmoothStep`, `Min`, `Max` (full table in F9).
  - Runtime uploads `current` values each frame with `EffectProperties::set` (`properties.rs:319`) on each particle entity of the instance (only when changed).
  - Init-time values read the property at spawn, update-time values every frame.
  - Parameter-driven colour: init `hdr_color` from `Mix(a, b, Param)` or the `Color` param directly, plus `ColorOverLifetime` with Modulate-style white keys for the alpha shape; or `SetColorModifier` (render) from an expression.
- **Spawn rate**: `rate: Binding<f32>` on CPU → `EffectSpawner.settings.set_count` (`spawn.rs:483`). Rate 0 ⇒ layer off.
- **Visibility ranges**: per layer `visible: (param, from, to, fade)` multiplies alpha / rate / intensity → tier looks that crossfade.
- Surface / shape / trail / afterimage / light / distortion values: `Binding<T>` evaluated on CPU each frame into material uniforms / components.

**Triggers**
- Declared per package: free names, any number. Effects of a trigger:
  - layers with `play: OnTrigger(name)` burst (`EffectSpawner::reset()` + active, *verify* `spawn.rs:814`);
  - parameter pulses: `current` of a parameter jumps to `to`, then returns to its target over `decay` seconds;
  - trail/afterimage `emit: Trigger(start, stop)` start/stop emitting.
- **Payload** (`trigger_at`): optional world `position`, `direction`, `strength`. Written into hanabi properties `trigger_pos` / `trigger_dir` / `trigger_strength` (layer space) before the burst, so a burst layer can spawn at the contact point and fly along the direction. Without payload: the layer's attach point and +Y.

**Editor UI**: **Parameters** panel: add/remove/rename parameters, kind, range, smoothing; a slider (Float) or colour picker (Color) per parameter driving the preview; "animate min→max→min" button. **Triggers** panel: add/remove, pulses, a Fire button per trigger, "auto-fire every N s", "fire at random point near target" for payload preview. Binding editor on every bindable value: toggle "constant / curve over parameter" + key list.

**MCP**: `preview_in_editor(name, stage, camera, params: {...}, trigger, motion)`; `analyze_project(params: [...sets...])` simulates at several parameter sets (default: every Float param at min, middle, max) and reports differences. The CPU evaluator learns `Param` and the new operators.

**Tests**: smoothing math; binding evaluation; Color params; property declaration in produced assets; shader generation with properties (`EffectShaderSources::generate` check); trigger pulses; payload property upload.

**AC**: dragging a parameter slider in the editor changes the effect smoothly; the game's `set_param` gives the same result; a trigger produces a visible pulse/burst; `trigger_at` spawns the burst at the given point.

### F3. Asset library and texture import
See 4.3. **AC**: a PNG imported by button, by drag & drop, and by MCP is copied into the game folder, appears in the Assets panel with thumbnail and channel view, is used by a particle layer and by a surface layer (with a chosen channel), and the exported package loads it in the game without the game passing a texture list; "Extract channel" produces a usable grayscale texture.

### F4. Surface layers (effect shader over the model, with volume)

**Approach: overlay copies.** For every matching mesh entity of the target's own model, spawn a sibling entity with the same `Mesh3d`, the same `SkinnedMesh` (shares joints → animates identically), and `MeshMaterial3d<OmagariEffectMaterial>`. Original materials untouched; layers stack; removal = despawn overlays. When meshes under the target appear or change (scene finished loading, equipment changed), overlays are updated.

**Shells (volume)**: a surface layer has 1..N shells; each shell is an overlay pushed outward along the normals by `offset` metres, with its own opacity/noise offset. One shell at 0 = coloured skin; 2–3 shells at 1–4 cm = a thick soft aura. Small offset also prevents z-fighting.

**Own vertex shader (required in phase 1).** Bevy's default mesh vertex shader passes only the animated (skinned) world position to the fragment shader. Patterns based on it slide over a moving body. Our vertex shader:
- does the normal skinning/morph path (reuse `bevy_pbr::skinning` / `mesh_functions`, *verify* imports in `bevy_pbr-0.19.1/src/render/mesh.wgsl`);
- passes the **rest-pose (pre-skinning) local position and normal** to the fragment shader, so triplanar noise sticks to the body;
- applies `offset` along the skinned normal (shells / inflate), scaled by fit.
Shader code is kept small and isolated in `src/shaders/`; it will need updates on Bevy upgrades (accepted by the user).

**Material `OmagariEffectMaterial`** (shared by surface, shape, trail, afterimage): `#[derive(Asset, AsBindGroup, TypePath, Clone)]`, one uniform block + up to 2 noise textures + 1 mask texture, each with a channel selector. `impl Material`: own `vertex_shader()` + `fragment_shader()`, `alpha_mode()` Blend or Add, `depth_bias()` small positive (tune for the orthographic camera), shadows and prepass off. One material asset per instance per layer (per shell), uniforms updated from bindings each frame.

Shader features (all bindable to parameters, each can be zero):
- colour (RGBA, HDR), blend Alpha | Add;
- 1–2 scrolling noise textures, mapping `Uv(scale)` | `Triplanar(scale)` (rest-pose space);
- rim / fresnel (colour, power, intensity);
- emissive multiplier (bloom);
- pulse (frequency, amount) + `flash` from trigger pulses;
- dissolve (threshold vs noise, glowing edge width) for appear/disappear;
- mask: object-space height gradient with `fill` (e.g. frost rising from the feet, energy filling a blade) and/or mask texture;
- fade in/out and layer visibility.

**Editor**: Surface layer panel with all values + bindings, mesh target picker (list of the preview model's meshes/materials), shell list; preview on the Model stage or Capsule fallback.
**MCP**: `check_project` validates targets against the preview model; `analyze_project` describes the layer in words (colour at parameter min/max, rim, scroll speed, pulse rate, opacity, blend, shells) and warns about opaque overlays hiding the model.
**Tests**: uniform packing (size/alignment), binding → uniform mapping, mesh matching / own-model rule, WGSL validation with naga in a unit test if feasible (*verify* `naga_oil` composition of Bevy imports), otherwise an editor smoke test.
**AC**: an animated character shows a scrolling surface pattern that sticks to the body (does not slide) and a glowing rim, identical in editor and game; two surface layers stack; a 3-shell layer looks thick; stop fades it out.

**Status (P4 part 1, done; see section 2).** Verified with agent screenshots on the user's Y Bot (`anim/tpose.glb` running with `anim/run.glb`): the triplanar pattern stays on the body (a still noise compared at clip times 0.22 s and 0.32 s: the patches on the head and back stay put while the body moves ~0.5 m), the rim glows, three additive shells give a ~3 cm glow around the silhouette, frost fills the legs to the hips at `frost` = 0.6, STOP fades the skin out over 0.6 s and leaves the model untouched, the status skin and the energy shell stack as two layers; GAME camera (orthographic, also zoomed to a 3.5 m height with the same 34.6 m distance) shows no z-fighting at shell offset 0; the CAPSULE stage works. Demos: `saved_effects/status_skin`, `energy_shell`, `frost` (`.omagari.ron`). Tests: `tests/runtime_surface.rs` (overlays per own mesh and shell, own-model rule, same mesh / skin / transform, meshes appearing / changing / going, despawn cleanup, uniforms from parameters, fade in, stop waits for the fade out, replace_package keeps the level), `tests/effect_shader.rs` (**WGSL validation without a GPU**: a headless `DefaultPlugins` app with rendering disabled loads Bevy's embedded shader libraries, Bevy's own `ShaderCache` composes the effect shader with naga_oil for six shader-def sets — rigid, skinned, skinned + morph + tangents, uniform-buffer skins, additive + tonemapping in the shader, positions only — and naga validates each; the `OmagariEffect` struct's field order / offsets / size are compared with `EffectUniforms`), uniform packing and value → uniform mapping (`effect_material` tests), format round trip / evaluation / renames (`format::surface` tests), MCP checks and descriptions (`mcp::surface_check` tests), recipes in `guide_recipes_pass_checks`. Choices: overlays are children of the mesh entities (the skinned mesh's transform is not used by skinning; children despawn with their mesh); one depth "push" of 2 mm toward the camera along the view ray in the vertex shader instead of a pipeline depth bias (Bevy 0.19 materials have none; constant biases differ wildly between the orthographic and perspective depth ranges); `fill` is measured over the covered meshes' rest-pose bounds (glTF bind space: metres, Y up) — for several rigid meshes in different node spaces the band is per mesh space; noise `contrast` added (the soft built-in noise needs it); the rim is multiplied by the noise pattern. Not done: hierarchy-tab buttons to add a mesh to a layer's targets (the inspector's picker does it); per-overlay pattern scale for skinned meshes of different skins in one layer (one value per layer).

### F4b. Shape layers
- A built-in shape mesh with `OmagariEffectMaterial`: `Sphere(radius)`, `Capsule(radius, length)`, `Cylinder(radius, height, open_ends)`, `Ring(inner, outer)` (flat), `Torus(major, minor)`, `Cone(radius, height)`, `Quad(size)`, `Arc(radius, width, angle)` (stylized slash). Later: mesh asset from the library.
- Attach like other layers; follow mode `Full | Position | PositionYaw` (a ground ring stays flat).
- Animated transform: `scale`, `rotation_speed` as bindings (incl. over instance age for one-shots: `age` input, see below); `duration` for one-shot shapes (shockwave grows and fades).
- Bindings get one extra input besides parameters: **`Age`** (seconds since the layer started), so one-shot shapes can animate without code.
- **AC**: shield bubble around a character, ground ring under it that stays flat while it moves, a one-shot expanding shockwave, an arc slash played by a trigger.

### F5. Model, animation and socket preview (editor)
- Stage **Model**: loads `preview.model` (`.glb/.gltf` from the game folder; later via the asset library) with `SceneRoot(GltfAssetLabel::Scene(0))`. Stage **Capsule** (0.7 × 1.8, origin at the centre) when no model is set.
- Animations: `Gltf::named_animations` (*verify*) → `AnimationGraph` with one clip, `AnimationPlayer` with loop, speed, pause, scrub. Clips may come from another `.glb` with the same skeleton (Mixamo animations are separate files in the user's `animation/` folder; *verify* no retargeting needed when bone names match).
- Hierarchy browser: tree of nodes/bones/meshes/materials; pick a node as `preview.attach_to`, as a socket node, or as segment endpoints.
- **Socket editor**: package sockets (name + optional node + offset) placed with a gizmo in the viewport, so weapon points (base, tip) need no change to the weapon file.
- Weapon preview: a second model attached to a chosen bone of the first (e.g. sword in the right hand), each can be a target.
- Segment emitter (particles): `SetPositionSegment(a, b, radius)` = `Mix(segment_a, segment_b, rand) + random disc offset`.
- Preview motion (F1) while the animation plays.
- MCP: `preview_in_editor(..., stage: "model")`; `list_assets`; `describe_model(path)` → node/bone/mesh/material and animation names.
- Licensing: do not commit Mixamo models to this repo; reference them from the game folder.
- **AC**: a character runs in the editor with a package attached; a weapon package follows a swing animation; segment particles spawn along the blade.

**Status (P3, done; see section 2).** Verified: `saved_effects/model_demo.omagari.ron` on the user's Y Bot (`anim/tpose.glb`) running with the clip of `anim/run.glb` — model on the floor with the player root 0.9 m above the feet, running the motion circle and facing it, a glow on `mixamorig:RightHand`, sparks along a virtual blade (sockets with offsets from the hand) and a burst fired at 0.3 s of every loop, GAME and ORBIT cameras (agent screenshots). A shield model held in the right hand as the weapon target worked (instance on the weapon, hand sockets reported as not below it). Tests: `model_info` on generated glTF fixtures, a headless Bevy app loading a fixture model without animations and playing a clip of another fixture file by node names (`editor::model` tests), MCP `describe_model` / `list_assets` / preview args, checks against a model, socket offset math, preview format serde defaults, the segment preset's shader generation. Choices: animation clips from other files play by Bevy's name-path targets (no retargeting; a model without animations gets targets added by the editor); the socket gizmo is a dot + label dragged in the camera plane, with numeric fields and nudge buttons (no axis handles); the weapon's offset ignores the bone's scale (Mixamo bones sit under a 0.01-scaled armature). Open for the user: weapon swing with a real weapon file; dragging sockets by hand. Not done: analysis puts model-node sockets at the target origin (rest positions from `ModelInfo` could place them), surface targets are not checked yet (no surface layers; `ModelInfo::has_mesh_or_material` is ready for P4).

### F5b. Trail layer (weapon swing trail)
- Two points (sockets `from`, `to`, e.g. blade base and tip) sampled every frame; samples older than `length` seconds are dropped.
- **Smoothing**: Catmull-Rom subdivision between samples (configurable `subdivisions`, default 4) so fast swings stay round.
- CPU-built ribbon mesh (two vertices per sample: base, tip), updated each frame (`Mesh` asset in `RENDER_WORLD | MAIN_WORLD` usage *verify*). UV: u = normalized age (0 new → 1 old), v = base → tip.
- Material `OmagariEffectMaterial` with `age` fade (opacity/colour curve over u), width falloff, additive glow, scrolling noise along u.
- Emission control: `emit: Always | Trigger(start, stop) | While(param, threshold)`. When emission stops, the remaining trail fades out over `length`.
- Optional sparks along the trail: a particle layer with `Segment` attach and `play: While(..)`.
- Built in world space (trails must stay in the world).
- **AC**: during a swing animation in the editor, a smooth glowing arc follows the blade and fades within `length`; nothing is drawn between swings; same in the game.

### F5c. Afterimage layer
- While emitting, every `interval` seconds snapshot the target model's meshes (rigid meshes: current global transform; skinned meshes: *verify* cost of copying joint matrices, or restrict v1 to rigid meshes like weapons) and spawn a ghost copy with `OmagariEffectMaterial`, fading over `lifetime`.
- Same `emit` control as trails. Cap on simultaneous ghosts.
- **AC**: a swinging sword leaves 4–6 fading ghost copies along its path.

### F6. Distortion layers (heat haze)
- Transmission pass: a material reads `view_transmission_texture` (`bevy_pbr-0.19.1/src/render/mesh_view_bindings.wgsl:102`) when `Material::reads_view_transmission_texture()` is true (`material.rs:186`).
- `OmagariDistortionMaterial`: samples it at `screen_uv + noise_offset * strength`, soft edge falloff, optional tint; drawn on a shape volume (capsule along a segment, sphere) or an overlay of a mesh.
- Camera: transmission enabled (*verify* component/field in `bevy_core_pipeline-0.19.1`), `Hdr`. **Spike first with the game's orthographic camera.** Fallback: "shimmer" particle recipe.
- Limitation: transparent hanabi particles are not distorted.
- **AC**: at full strength the air around the attach point visibly wobbles in editor and game; no cost when strength is 0.

### F7. Light layers
- `PointLight` child at the attach point; `color`, `intensity`, `range` bindings; optional flicker; fades with the instance; shadows off by default; global cap on Omagari lights (oldest/weakest dropped).
- **AC**: light grows with its parameter; fades out on stop.

### F8. MCP and guide (every phase)
- MCP tools accept v2 (and v1, migrated). `write_project` validates v2.
- New/changed tools: `import_asset`, `list_assets`, `extract_channel`, `describe_model`, `preview_in_editor` (+ `stage`, `camera`, `params`, `trigger`, `motion`), `analyze_project` (+ parameter sets).
- Preview request file becomes a small JSON object (name, stage, camera, params, trigger, motion); the old two-line form is still accepted.
- Guide `docs/CLAUDE_VFX_GUIDE.md`: sections `parameters`, `triggers`, `assets`, `attach`, `surfaces`, `shapes`, `trails`, `afterimages`, `lights`, `distortion`; recipes (tested, zero warnings): status aura (particles + surface), slow-style cue, intensity progression 0→1, shield bubble, ground ring, slash trail, afterimages, hit burst at contact point.
- Human docs: `README.md` (game integration with the new API), `docs/MCP.md`.

### F9. bevy_hanabi coverage (all 3D features)

Inventory from `bevy_hanabi-0.19.0/src` (`modifier/*.rs`, `asset.rs`, `graph/expr.rs`) and `examples/`. "Now" = exposed today (updated after P2b). Every row except "skip" is implemented; order is P2b first, then P3b for items needing the model preview. The coverage test is `tests/hanabi_coverage.rs`; skips with reasons are in `src/format/coverage.rs`.

**Modifiers**

| hanabi | Kind | Now | Phase | Notes / use |
|---|---|---|---|---|
| `SetAttributeModifier`, `InheritAttributeModifier` | init | yes | – | |
| `SetPositionSphereModifier`, `SetPositionCircleModifier` | init | yes | – | |
| `SetPositionCone3dModifier` | init | yes (P2b) | P2b | cones: fountains, thrusts. Init only (hanabi lists Init+Update, but its code reads the init-only spawner transform); always fills the volume |
| `SetVelocitySphere/Circle/TangentModifier` | init | yes | – | |
| `AccelModifier`, `LinearDragModifier`, `ConformToSphereModifier` | update | yes | – | |
| `RadialAccelModifier` | update | yes (P2b) | P2b | attract/repel: vortex, implosion |
| `TangentAccelModifier` | update | yes (P2b) | P2b | swirl: auras, tornado |
| `KillSphereModifier`, `KillAabbModifier` | update | yes (P2b) | P2b | kill inside/outside a volume (sphere radius written as `radius_expr`) |
| `EmitSpawnEventModifier` | update | yes | – | not in hanabi's `register_modifiers()`; `register_missing_hanabi_modifiers` adds it |
| `ParticleTextureModifier` | render | yes (P2b) | P2b | all three `ImageSampleMapping`s |
| `SetColorModifier`, `SetSizeModifier` | render | yes (P2b) | P2b | hanabi 0.19 takes constants (`CpuValue`), not expressions; a `Uniform` range is one value for all particles (no per-particle seed at render time). Parameter-driven colour: init `hdr_color` from `Param` |
| `ColorOverLifetimeModifier`, `SizeOverLifetimeModifier` | render | yes | – | hanabi 0.19 gradients are linear only (no options); `SizeOverLifetime.screen_space_size` is ignored by hanabi (skipped; use ScreenSpaceSize) |
| `OrientModifier` | render | yes (P2b) | P2b | all `OrientMode`s + `rotation_expr` (radians; used by ParallelCameraDepthPlane / FaceCameraPosition) |
| `FlipbookModifier` | render | yes (P2b) | P2b | texture `grid` (rows, cols) or its own; `sprite_index` (i32) from the prebuilt update SetAttribute |
| `RoundModifier` | render | yes (P2b) | P2b | rounded quads (`roundness_expr`) |
| `ScreenSpaceSizeModifier` | render | yes (P2b) | P2b | constant pixel size |
| Ribbons (`Attribute::RIBBON_ID`, `prev`/`next`, `examples/ribbon.rs`) | init + render | no | P3b | magic streaks; needs `MotionIntegration::None`; editor preset |
| Mesh particles (`EffectAsset::mesh`, `asset.rs:335,643`) | asset | no | P3b | debris, shards, leaves; mesh from the asset library |
| Any other modifier found in the 0.19 sources | | none | – | the registry lists 25 modifiers, all mapped; `SKIPPED_MODIFIERS` is empty |
| `Expr::TextureSample` (texture lookups inside expressions) | expr | skip | P3b/P4 | needs more texture slots than a particle layer has |

**Effect asset settings**

| Setting | Now | Phase | Notes |
|---|---|---|---|
| `simulation_space` | yes (P1) | P1 | per layer |
| `alpha_mode` (`Blend`, `Premultiply`, `Add`, `Multiply`, `Mask(cutoff)`, `Opaque`) | yes (P2) | P2b | Add = additive glow; the Mask cutoff is a number (an expression cutoff is skipped) |
| `motion_integration` | yes (P2b) | P2b | `None` for ribbons / expression-positioned particles; also silences hanabi's "missing VELOCITY" log |
| `simulation_condition` | yes (P2b) | P2b | `Always` for off-screen simulation |
| `prng_seed` | yes (P2b) | P2b | reproducible looks (`Option<u32>`, None = hanabi's 0) |
| spawner `once` / `rate` / `burst`, `Uniform` counts | yes (P2b) | P2b | editor presets = hanabi's constructors, counts (also ranges) kept |
| draw ordering between effects (`examples/ordering.rs`) | no | P3b | stable layering |
| `z_layer_2d`, 2D rendering, render layers / multi-camera | – | skip | 2D / not needed |

**Expressions** (done in P2b): a **table-driven** model over hanabi's `BuiltInOperator`, `UnaryOperator`, `BinaryOperator`, `TernaryOperator` (+ `Cast`, attributes, properties, parameters, literals of every scalar/vector `Value` type; matrices skipped: no input takes one). New operator = one enum variant + one `OPERATORS` row (name, args, hint, menu group) + arms in `name`, `args`, `args_mut`, `produce`, the analysis evaluator (type rule + CPU value) and `coverage.rs`. Not usable with hanabi 0.19 (kept loadable, reported by the check): integer/bool random values, `AlphaCutoff`.

**Coverage test** (`tests/hanabi_coverage.rs`, P2b): hanabi's modifiers come from its reflection registry after `register_modifiers()` (+ `register_missing_hanabi_modifiers`, for EmitSpawnEvent); each must be produced by a default Omagari modifier or be in `SKIPPED_MODIFIERS`. Operator enums and small enums are matched exhaustively in `format/coverage.rs` (compile-time), and the test checks that each mapped Omagari operator produces exactly that hanabi operator. Updating hanabi then surfaces new features as a compile error or a failing test. The same file round-trips a project with every modifier and setting through the runtime loader and generates every layer's shaders.

**Rules**
- One pattern per modifier: data struct in `src/format/particles.rs`, UI in `src/editor/`, produce arm. Formerly hard-coded render settings become normal removable modifiers with defaults reproducing the old look.
- Round-trip test per modifier: build a project using it, save, load through the runtime loader, produce, compare.
- MCP analysis: the CPU simulator simulates every update modifier and operator, or reports the layer as "not simulated" explicitly (never silently wrong). Render-only features are described, not simulated.
- Guide: every feature gets a line; tested recipes use additive blending, a flipbook, TangentAccel/RadialAccel, a cone (P2b: "Cone fountain", "Swirl aura", "Flipbook"), a ribbon (P3b).

**AC**: every non-skipped item can be added, edited, saved, loaded in the game and previewed in the editor; `check_project`/`analyze_project` handle it; coverage test passes.

### F10. Editor UX
Follow the current egui style; use common app practice:
- **Layout**: top toolbar (project, save/export, stage, camera, backdrop); left: **Layers** list (add/duplicate/delete/reorder, visibility toggle per layer); right: **Inspector** for the selected layer; tabs/panels for **Parameters**, **Triggers**, **Assets**, **Sockets**; bottom or overlay: playback (play/stop/restart instance, time scale, model animation controls).
- **Live preview**: edits rebuild the preview instance automatically (debounced ~150 ms); RENDER button stays as a manual refresh.
- **Undo/redo**: snapshot-based (serialize project on each committed edit; Ctrl+Z / Ctrl+Y), at least 100 steps.
- **Tooltips** in plain words on every value; units shown (m, s, m/s).
- **Shortcuts**: Ctrl+S save, Ctrl+E export, Ctrl+Z/Y, Delete, Ctrl+D duplicate, F focus camera.
- **Dirty state**: unsaved marker, confirm before closing/loading with unsaved changes.
- **Errors**: validation from `check_project` shown in the UI next to the field, not only in the MCP output.
- **New from recipe**: create a package from a guide recipe.
- **Camera presets** (render profile in `omagari_settings.ron`): `Orbit` (current) and `Game` (projection, orthographic height, yaw/pitch, tonemapping, bloom, exposure, background). Defaults for `Game` match the game camera (orthographic, true isometric 45° / 35.26°, ~16 units visible height, `Hdr`, bloom).

---

## 6. Game integration (game agent, after user confirmation)

Facts provided by the user about the game (game repo, likely `C:\bevy\justForFun`, *confirm with the user*):
- Player capsule radius 0.35, height 1.5 (`player/mod.rs:56-59`); visible model Mixamo Y Bot ≈ 1.8 m; root 0.9 above the feet (`player_rig.rs:19`); metres, Y up.
- Camera: orthographic true-isometric (45° yaw, ≈35.26° pitch), offset (20,20,20), ≈16 world units visible vertically (`world/camera.rs:31-34`). Effects must read at ~1/10 screen height per character.
- Statuses (example usage): Poison 4 s, damage every 1 s, re-contact resets the timer, poison zone (2.5 m, 8 s) re-applies every 0.5 s; Slow half speed 3 s, own cyan cue; Ignite orange, rises. Placeholder poison colour `srgb(0.65, 0.08, 0.95)` (`combat_periodic_status/visual.rs:20`). Enemies can be affected.
- Movement: walk 7.2 m/s, dodge 16 m/s.
- Weapons are separate models with their own effects.
- The game uses bevy_hanabi 0.19 and Bevy 0.19.1.

Integration pattern (game-side code, not in Omagari):
- Add `OmagariPlugin` + `HanabiPlugin`; camera `Hdr` + `Bloom` (+ transmission if distortion is used).
- Assets: `assets/vfx/*.vfx.ron`, `assets/vfx/textures/*`; Omagari's game assets folder points at the game's `assets/`.
- Status pattern: store the instance per (target, status); on apply: spawn, or `restart` if Stopping; on damage tick: `trigger("tick")`; on expire: `stop`. Same code for player and enemies.
- Progression pattern: the game maps its progress to whatever parameters the package declares (`set_param`), smoothing animates changes.
- Weapon pattern: package on the weapon entity; `trigger("swing_start")` / `trigger("swing_end")` from the attack state for trails/afterimages; `trigger_at("hit", contact)` on contact.

---

## 7. Roadmap

Each phase lands as its own commit series on branch `vfx-system`, with `cargo build --release --features editor` and `cargo test --release --features editor` passing, and a demo package in `saved_effects/` or `examples/`. Guide/MCP/docs are updated in every phase (F8).

| Phase | Content | Depends on | Done when |
|---|---|---|---|
| **P0a Data model split** | move the particle data model out of the editor into `src/format/` (serde, no egui); UI impls into `src/editor/`; `produce()` in the library; delete `src/complex.rs`. **No behaviour change.** | – | build + tests pass; editor and MCP behave as before; produced assets identical for all examples |
| **P0b Format v2 + runtime slice** | ProjectV2 (`#[serde(default)]`), v1 migration, package loader (`.vfx.ron` / `.omagari.ron`), `game` asset source + settings, minimal instance (particles only, attach Target, Local space, stop, `VfxFinished`), editor saves v2 and previews through the runtime | P0a | AC of 4.5; a package spawned by a test app follows a moving entity and stops |
| **P1 Instances complete** | rest of F1 (sockets, segments, fit, restart, hot reload, `spawn_vfx_at`, legacy `EffectComplex` returns entities + graceful stop), preview motion | P0b | AC of F1 |
| **P1b Assets + UX base** | F3 asset library + import (dialog, drag & drop, MCP, channel view, extract channel); F10 layout, live preview, undo/redo, dirty state, camera presets | P0b | AC of F3; F10 basics usable |
| **P2 Parameters & triggers** | F2 complete (runtime, expressions, panels, MCP, guide) | P1 | AC of F2 |
| **P2b hanabi coverage** | F9 rows marked P2b, expression table, coverage test | P2 | AC of F9 for P2b rows |
| **P3 Model preview** | F5 (glTF, animations, hierarchy, sockets editor, weapon preview, segment emitter, `describe_model`) — **done** | P1 (can run parallel to P2/P2b) | AC of F5 |
| **P3b hanabi coverage 2** | ribbons, mesh particles, draw ordering | P2b, P3 | ribbon recipe works on the animated model |
| **P4 Effect shader** | F4 surface layers (own vertex shader, shells) — **part 1 done** — + F4b shape layers | P2, P3 | AC of F4 and F4b |
| **P5 Trails + afterimages** | F5b, F5c | P4 | AC of F5b, F5c |
| **P6 Lights** | F7 | P4 | AC of F7 |
| **P7 Distortion** | F6 (spike with ortho camera first) | P4 | AC of F6 |
| **G1..G3 Game** | integrate after P2, P4, P5 (user confirms repo and scope first) | | 6.x patterns working in the game |

### Verification per phase
- `cargo build --release --features editor` and `cargo test --release --features editor`.
- MCP: `check_project`, `analyze_project` on the phase demo.
- Visual checks are done by the user in the editor (agents cannot see the screen); each phase lists what to look at.

---

## 8. Risks and open questions

**Risks**
- **Bevy upgrades** break custom materials/WGSL (F4–F7). Accepted; shaders kept small and isolated, smoke test per phase.
- **Own vertex shader** must follow Bevy's mesh/skinning shader closely; verify early in P4 with a skinned Mixamo model.
- **hanabi limits**: gradients cannot use properties (use init colours/SetColor), spawn count is CPU-side, particles can't pick texture channels (extract channel), no distortion in particles.
- **Transmission + orthographic camera** (P7): spike first, shimmer fallback.
- **Overlay sorting / z-fighting** with the orthographic camera: depth bias + shell offset.
- **Trail mesh updates** every frame: keep vertex counts small (≤ 200 per trail).
- **glTF names** (sockets, targets): confirm Bevy 0.19 naming in P3.
- **Performance**: one draw per matched mesh per shell per surface layer per instance; measure with many enemies.

**Open questions (ask the user when the phase starts)**
1. Game repo path and when to start game integration (G phases).
2. Afterimages on skinned (animated) meshes, or rigid meshes (weapons) only?
3. Should statuses fade out on death or stop instantly? (game side)
