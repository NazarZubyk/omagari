//! Checks and descriptions of distortion layers (P7, F6) for `check_project`
//! and `analyze_project`. Distortion is not simulated: how far the picture
//! moves is worked out from the bindings (in pixels of a 1080 px tall view)
//! and described in words, with what it can and cannot bend.

use std::fmt::Write;
use std::path::Path;

use bevy::prelude::*;
use omagari::format::*;

use super::analysis::{fmt_num, param_values, Diagnostics, ParamSet, PreviewModelNodes};
use super::shape_check::{attach_words, between_when, between_words, binding_words, check_attach_sockets, check_between, check_keys, follow_text};
use super::surface_check::{lookup, preview_target_model};

/// Strengths below this (fraction of the view height, ≈ 1 px of a 1080 px
/// view) hardly show.
pub const WEAK_DISTORTION: f32 = 0.001;
/// View height the descriptions give pixels for.
const VIEW_PX: f32 = 1080.0;
/// Surface-source offsets above this (metres) float far from the body.
const MAX_SURFACE_OFFSET: f32 = 0.15;

/// Every value a number binding can take (the constant, or every key).
fn values_of(b: &Binding<f32>) -> Vec<f32> {
    match b {
        Binding::Const(v) => vec![*v],
        Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
    }
}

fn scale_values(b: &Binding<Vec3>) -> Vec<Vec3> {
    match b {
        Binding::Const(v) => vec![*v],
        Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
    }
}

/// Pixels of a 1080 px tall view for a strength.
fn px(strength: f32) -> String {
    fmt_num((strength * VIEW_PX).round())
}

/// What the picture does at one strength, in words.
fn strength_text(strength: f32) -> String {
    if strength <= 0.0 {
        "nothing (not drawn, no cost)".into()
    } else {
        let kind = if strength < WEAK_DISTORTION {
            "barely visible"
        } else if strength <= 0.012 {
            "a heat shimmer"
        } else if strength <= STRONG_DISTORTION {
            "a strong ripple"
        } else {
            "tearing"
        };
        format!("moves the picture by up to {} of the view height (~{} px on a 1080 px view): {}", fmt_num(strength), px(strength), kind)
    }
}

/// The noise slots: `(label, noise)`.
fn noises(layer: &DistortionLayerDef) -> Vec<(&'static str, &DistortionNoiseDef)> {
    let mut out = vec![("noise", &layer.noise)];
    if let Some(n) = &layer.noise_b {
        out.push(("noise_b", n));
    }
    out
}

/// The ages a description samples: over the one-shot / repeat duration, or
/// up to the last `Age` key.
fn sample_ages(layer: &DistortionLayerDef) -> Vec<f32> {
    let span = match layer.duration {
        Some(d) if d > 0.0 => Some(d),
        _ => layer.age_bindings().iter().map(|(_, last)| *last).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b)))),
    };
    let Some(span) = span.filter(|s| *s > 0.0) else { return Vec::new() };
    let mut ages: Vec<f32> = [0.0, 0.25, 0.5, 0.75].iter().map(|f| f * span).collect();
    ages.push(if layer.is_one_shot() { span * 0.98 } else { span });
    ages
}

/// Whether a layer is drawn after the distortion and therefore not bent:
/// every transparent layer (particles that are not Opaque / Mask, surface,
/// shape, trail and afterimage layers).
fn is_transparent(layer: &LayerDef) -> bool {
    match layer {
        LayerDef::Particles(p) => !matches!(p.effect.alpha_mode, AlphaModeDef::Opaque | AlphaModeDef::Mask(_)),
        LayerDef::Surface(_) | LayerDef::Shape(_) | LayerDef::Trail(_) | LayerDef::Afterimage(_) => true,
        LayerDef::Light(_) | LayerDef::Distortion(_) => false,
    }
}

/// Every check of the distortion layers.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn check_distortion_layers(d: &mut Diagnostics, project: &ProjectV2, model: Option<&PreviewModelNodes>, assets_root: &Path) {
    let has_distortion = project.distortion_layers().next().is_some();
    if has_distortion {
        let transparent: Vec<&str> = project.layers.iter().filter(|l| is_transparent(l)).map(|l| l.name()).collect();
        if !transparent.is_empty() {
            d.notes.push(format!(
                "the distortion bends only opaque things (the floor, walls, models): this package's {} {} drawn after it and {} sharp, even where {} behind the distortion",
                transparent.iter().map(|n| format!("\"{}\"", n)).collect::<Vec<_>>().join(", "),
                if transparent.len() == 1 { "is" } else { "are" },
                if transparent.len() == 1 { "stays" } else { "stay" },
                if transparent.len() == 1 { "it is" } else { "they are" }
            ));
        }
    }
    let surface_model = preview_target_model(project, assets_root);
    for (index, layer) in project.distortion_layers() {
        let name = layer.name.as_str();
        if name.trim().is_empty() {
            d.errors.push("a distortion layer has an empty name".into());
        }
        if project.layers.iter().enumerate().any(|(i, l)| i != index && l.name() == name) {
            d.error(name, "duplicate layer name; names must be unique");
        }

        // Source.
        match &layer.source {
            DistortionSourceDef::Shape { shape, .. } => {
                let segment_capsule = layer.is_segment_capsule();
                for (what, value) in shape.dimensions() {
                    if segment_capsule && what == "length" {
                        continue;
                    }
                    if !(value > 0.0) {
                        d.error(name, format!("{} {} {} must be above 0 metres: nothing is drawn", shape.kind_name(), what, fmt_num(value)));
                    }
                }
                let scales = scale_values(&layer.scale);
                if scales.iter().any(|s| s.min_element() < 0.0) {
                    d.warn(name, "scale has a value below 0: the volume turns inside out and is not drawn");
                }
                let biggest = scales.iter().map(|s| shape.size() * s.max_element()).fold(0.0f32, f32::max);
                if biggest > HUGE_SHAPE && !segment_capsule {
                    d.warn(name, format!("the volume gets ~{} m across: huge for a character effect (the game camera shows ~16 m); check the units", fmt_num(biggest)));
                }
                if shape.is_flat() {
                    d.warn(
                        name,
                        format!("a flat {} bends only where it faces the camera and vanishes edge-on; a sphere or capsule works from every side", shape.kind_name().to_lowercase()),
                    );
                }
                check_attach_sockets(d, project, name, &layer.attach, model, "the distortion");
                if let (AttachDef::Segment { .. }, false) = (&layer.attach, segment_capsule) {
                    d.note(name, "attached to a segment but the shape is not a capsule: it sits at the segment's middle (only a capsule spans the two sockets)");
                }
            }
            DistortionSourceDef::Surface { targets, offset } => {
                if layer.attach != AttachDef::Target {
                    d.note(name, "a Surface source covers the target's own meshes: its attach is not used");
                }
                if *offset < 0.0 {
                    d.warn(name, format!("surface offset {} m is inside the model: the model hides it", fmt_num(*offset)));
                } else if *offset > MAX_SURFACE_OFFSET {
                    d.warn(name, format!("surface offset {} m floats far from the body (an aura hugs it at 0.02–0.06 m)", fmt_num(*offset)));
                } else if *offset < 0.005 {
                    d.warn(name, "surface offset below 5 mm: the copies lie on the model's own surface and bend mostly the model; push them out 0.02–0.06 m");
                }
                if let MeshTargetsDef::Meshes(names) | MeshTargetsDef::Materials(names) = targets {
                    if names.is_empty() {
                        d.warn(name, "targets list is empty: the layer covers nothing (use AllMeshes for every mesh)");
                    }
                    for target in names {
                        match &surface_model {
                            Some((path, info)) if !info.has_mesh_or_material(target) => d.error(
                                name,
                                format!("target \"{}\" is not a mesh, mesh entity or material of the preview model {} (describe_model lists them)", target, path),
                            ),
                            Some(_) => {}
                            None => d.note(name, format!("target \"{}\" must be a mesh or material name of the game's model (no preview model to check it)", target)),
                        }
                    }
                }
            }
        }

        // Strength.
        check_keys(d, name, "strength", layer.strength.keys().map(|k| k.iter().map(|k| k.0).collect()), layer.strength.param(), project);
        let strengths = values_of(&layer.strength);
        if strengths.iter().any(|s| *s < 0.0) {
            d.error(name, "strength has a value below 0");
        }
        let peak = strengths.iter().cloned().fold(0.0f32, f32::max);
        if !strengths.is_empty() && peak <= 0.0 {
            d.warn(name, "strength is 0 everywhere: the layer never bends anything (and is never drawn)");
        } else if peak > 0.0 && peak < WEAK_DISTORTION {
            d.warn(name, format!("strength at most {}: under ~1 px on a 1080 px view, too weak to see (it is a fraction of the view height; heat haze is 0.005–0.012)", fmt_num(peak)));
        }
        if peak > STRONG_DISTORTION {
            d.warn(
                name,
                format!(
                    "strength up to {} of the view height (~{} px on a 1080 px view): the picture tears instead of shimmering; heat haze is 0.005–0.012, a strong ripple 0.02–0.03",
                    fmt_num(peak),
                    px(peak)
                ),
            );
        }

        // Noise.
        for (label, noise) in noises(layer) {
            if noise.has_texture() {
                match project.asset(&noise.texture.asset) {
                    None => d.error(name, format!("{} texture \"{}\" is not in the assets list", label, noise.texture.asset)),
                    Some(asset) => match asset.kind.as_texture() {
                        None => d.error(name, format!("{} \"{}\" is not a texture asset", label, noise.texture.asset)),
                        Some(t) if t.wrap() == TextureWrap::Clamp => d.warn(
                            name,
                            format!(
                                "{} texture \"{}\" wraps with Clamp: the noise smears at the edges of every square; give the texture the purpose Noise (Repeat)",
                                label, noise.texture.asset
                            ),
                        ),
                        Some(_) => {}
                    },
                }
            } else if label == "noise_b" {
                d.warn(name, "noise_b has no texture: it adds nothing (pick a texture or remove it)");
            }
            if !(noise.scale > 0.0) {
                d.warn(name, format!("{} scale {} must be above 0 (repeats per metre)", label, fmt_num(noise.scale)));
            }
            if !(0.0..=1.0).contains(&noise.weight) {
                d.warn(name, format!("{} weight {} is outside 0..1", label, fmt_num(noise.weight)));
            }
            if noise.contrast < 0.0 {
                d.warn(name, format!("{} contrast {} is below 0", label, fmt_num(noise.contrast)));
            }
        }
        let moves = noises(layer).iter().any(|(_, n)| n.has_texture() && n.weight > 0.0) || layer.refraction > 0.0;
        if !moves && peak > 0.0 {
            d.warn(
                name,
                "no noise texture and no refraction: nothing moves the picture (only the tint shows); pick a noise texture (e.g. the built-in noise) or set refraction",
            );
        }
        if !(0.0..=1.0).contains(&layer.edge_softness) {
            d.warn(name, format!("edge_softness {} is outside 0..1", fmt_num(layer.edge_softness)));
        }
        if !(0.0..=1.0).contains(&layer.refraction) {
            d.warn(name, format!("refraction {} is outside 0..1", fmt_num(layer.refraction)));
        }

        // Tint.
        match &layer.tint {
            Binding::Param(param, keys) => match project.param(param).map(|p| p.kind()) {
                None => d.error(name, format!("tint reads \"{}\", which is not a parameter of this package", param)),
                Some(ParamKind::Color) => {
                    if !keys.is_empty() {
                        d.warn(name, format!("tint reads the Color parameter \"{}\" directly; its keys are not used", param));
                    }
                }
                Some(ParamKind::Float) => check_keys(d, name, "tint", Some(keys.iter().map(|k| k.0).collect()), Some(param), project),
            },
            Binding::Age(keys) => check_keys(d, name, "tint", Some(keys.iter().map(|k| k.0).collect()), None, project),
            Binding::Const(_) => {}
        }
        let tints: Vec<Vec4> = match &layer.tint {
            Binding::Const(c) => vec![*c],
            Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
        };
        if tints.iter().any(|t| t.w > 1.0 || t.w < 0.0) {
            d.warn(name, "tint amount (alpha) is outside 0..1 (it is clamped)");
        }
        check_keys(d, name, "scale", layer.scale.keys().map(|k| k.iter().map(|k| k.0).collect()), layer.scale.param(), project);

        // Play, duration and age.
        match &layer.play {
            PlayDef::OnTrigger(trigger) => {
                if project.trigger(trigger).is_none() {
                    d.error(name, format!("plays on trigger \"{}\", which is not a trigger of this package", trigger));
                }
                if layer.duration.is_none() {
                    d.warn(name, "plays on a trigger but has no duration: after the first trigger it stays on until the effect stops; set duration (seconds) for a ripple");
                }
            }
            PlayDef::Between { .. } => check_between(d, project, name, &layer.play),
            PlayDef::While(param, _) => match project.param(param).map(|p| p.kind()) {
                None => d.error(name, format!("play While reads \"{}\", which is not a parameter of this package", param)),
                Some(ParamKind::Color) => d.error(name, format!("play While reads \"{}\", a Color parameter; it needs a Float one", param)),
                Some(ParamKind::Float) => {}
            },
            PlayDef::OnStart if layer.duration.is_none() => {
                d.note(name, "OnStart without a duration bends like Loop (until the effect stops); set duration for a one-shot")
            }
            _ => {}
        }
        if let Some(duration) = layer.duration {
            if !(duration > 0.0) {
                d.error(name, format!("duration {} must be above 0 seconds", fmt_num(duration)));
            } else {
                if layer.is_one_shot() && layer.fade.fade_in + layer.fade.fade_out > duration + 1e-4 {
                    d.warn(
                        name,
                        format!(
                            "fade in {} + out {} s is longer than the duration {} s: it never reaches full strength",
                            fmt_num(layer.fade.fade_in),
                            fmt_num(layer.fade.fade_out),
                            fmt_num(duration)
                        ),
                    );
                }
                for (what, last) in layer.age_bindings() {
                    if last > duration + 1e-4 {
                        d.warn(
                            name,
                            format!(
                                "{} curve over age has keys up to {} s, past the duration {} s: {}",
                                what,
                                fmt_num(last),
                                fmt_num(duration),
                                if layer.is_one_shot() { "the layer is gone before they are reached" } else { "the age starts over before they are reached" }
                            ),
                        );
                    }
                }
            }
        } else if !layer.age_bindings().is_empty() && !matches!(layer.play, PlayDef::OnTrigger(_)) {
            d.note(name, "curves over age run once from the start and then hold their last key; give the layer a duration to repeat them");
        }
        if layer.fade.fade_in < 0.0 || layer.fade.fade_out < 0.0 {
            d.error(name, "fade in / out must be 0 or more seconds");
        }
        if let Some(v) = &layer.visible {
            match project.param(&v.param).map(|p| p.kind()) {
                None => d.error(name, format!("visible reads \"{}\", which is not a parameter of this package", v.param)),
                Some(ParamKind::Color) => d.error(name, format!("visible reads \"{}\", a Color parameter; it needs a Float one", v.param)),
                Some(ParamKind::Float) => {}
            }
            if v.from > v.to {
                d.error(name, format!("visible from {} is above to {}", fmt_num(v.from), fmt_num(v.to)));
            }
        }
    }
}

fn play_text(project: &ProjectV2, layer: &DistortionLayerDef) -> String {
    let fades = format!("fades in {} s, out {} s", fmt_num(layer.fade.fade_in), fmt_num(layer.fade.fade_out));
    match (&layer.play, layer.one_shot_duration()) {
        (PlayDef::OnStart, Some(d)) => format!(
            "OnStart one-shot: bends for {} s when the effect starts ({} inside that time), then is gone; the effect finishes by itself if every layer is a one-shot",
            fmt_num(d),
            fades
        ),
        (PlayDef::OnTrigger(t), Some(d)) => {
            let known = if project.trigger(t).is_some() { "" } else { " (NOT a trigger of this package)" };
            format!(
                "OnTrigger(\"{}\"){} one-shot: each time the game fires \"{}\" it plays {} s from age 0 ({} inside that time; a trigger during the play starts it over; with trigger_at it plays at the payload position)",
                t, known, t, fmt_num(d), fades
            )
        }
        (PlayDef::OnTrigger(t), None) => format!("OnTrigger(\"{}\") without a duration: starts at the first trigger and stays until the effect stops ({})", t, fades),
        (PlayDef::While(p, threshold), _) => format!(
            "While(\"{}\", {}): bends while {} ≥ {} ({}; the age starts over each time it switches on)",
            p,
            fmt_num(*threshold),
            p,
            fmt_num(*threshold),
            fades
        ),
        (PlayDef::Between { .. }, _) => format!(
            "{}: bends {} ({}; the age starts over each time it switches on)",
            between_words(&layer.play),
            between_when(project, &layer.play),
            fades
        ),
        (PlayDef::Loop | PlayDef::OnStart, _) => format!(
            "{}: bends while the effect runs ({} with the effect{})",
            if layer.play == PlayDef::Loop { "Loop" } else { "OnStart (no duration)" },
            fades,
            layer.duration.map(|d| format!("; the age starts over every {} s", fmt_num(d))).unwrap_or_default()
        ),
    }
}

/// The volume in words.
fn source_text(project: &ProjectV2, layer: &DistortionLayerDef) -> String {
    match &layer.source {
        DistortionSourceDef::Shape { shape: ShapeDef::Capsule { radius, .. }, .. } if layer.is_segment_capsule() => format!(
            "a capsule of radius {} m spanning {} (its length follows the sockets)",
            fmt_num(*radius),
            attach_words(project, &layer.attach).replace("the middle of the segment", "the segment").replace(", its Y axis along the segment", "")
        ),
        DistortionSourceDef::Shape { shape, follow, .. } => {
            format!("{} at {}; follow {}", shape.describe(), attach_words(project, &layer.attach), follow_text(*follow))
        }
        DistortionSourceDef::Surface { targets, offset } => format!(
            "the air around {} of the target's own model: copies of them pushed {} m outward (they move with the body)",
            match targets {
                MeshTargetsDef::AllMeshes => "every mesh".to_string(),
                MeshTargetsDef::Meshes(n) => format!("the meshes {}", n.join(", ")),
                MeshTargetsDef::Materials(n) => format!("the meshes with the materials {}", n.join(", ")),
            },
            fmt_num(*offset)
        ),
    }
}

fn noise_text(n: &DistortionNoiseDef) -> String {
    if !n.has_texture() {
        return "no texture (off)".into();
    }
    let mapping = match n.mapping {
        NoiseMappingDef::Triplanar => "per metre (triplanar)",
        NoiseMappingDef::Uv => "per UV square",
    };
    format!(
        "\"{}\" ({}), {} {}, scrolling ({}, {}) squares/s{}{}",
        n.texture.asset,
        n.texture.channel.name(),
        fmt_num(n.scale),
        mapping,
        fmt_num(n.scroll.x),
        fmt_num(n.scroll.y),
        if n.contrast != 1.0 { format!(", contrast {}", fmt_num(n.contrast)) } else { String::new() },
        if n.weight != 1.0 { format!(", weight {}", fmt_num(n.weight)) } else { String::new() }
    )
}

/// Size of a shape source at a scale, in words.
fn size_text(layer: &DistortionLayerDef, scale: Vec3) -> Option<String> {
    let shape = layer.source.shape().filter(|_| !layer.is_segment_capsule())?;
    Some(format!("~{} m across", fmt_num(shape.size() * scale.max_element())))
}

/// The description of a distortion layer for `analyze_project`.
pub fn describe_distortion_layer(project: &ProjectV2, index: usize, layer: &DistortionLayerDef, out: &mut String) {
    let _ = writeln!(out, "\n## [{}] \"{}\" — distortion layer: {}", index, layer.name, source_text(project, layer));
    let _ = writeln!(
        out,
        "  bends: what is behind it, opaque things only (the floor, walls, models); transparent particles and Omagari's see-through layers are drawn after it and stay sharp"
    );
    let _ = writeln!(out, "  play: {}", play_text(project, layer));
    let _ = writeln!(out, "  strength: {} (fraction of the view height)", binding_words(&layer.strength, |v: f32| fmt_num(v), ""));
    if matches!(layer.source, DistortionSourceDef::Shape { .. }) && layer.scale != Binding::Const(Vec3::ONE) {
        let _ = writeln!(out, "  scale: {}", binding_words(&layer.scale, |v: Vec3| format!("({}, {}, {})", fmt_num(v.x), fmt_num(v.y), fmt_num(v.z)), ""));
    }
    let _ = writeln!(out, "  noise: {}", noise_text(&layer.noise));
    if let Some(n) = &layer.noise_b {
        let _ = writeln!(out, "  noise B: {}", noise_text(n));
    }
    let _ = writeln!(
        out,
        "  edge: {}",
        match layer.edge {
            DistortionEdgeDef::Soft => format!(
                "Soft — strongest where the surface faces the camera (the middle, a capsule's axis), nothing at the outline (full from {}% in)",
                fmt_num((layer.edge_softness * 100.0).round())
            ),
            DistortionEdgeDef::Rim => format!(
                "Rim — a band inside the outline, peaking {}% in, fading to the outline and to the middle (a ring on a sphere)",
                fmt_num((layer.edge_softness * 100.0).round())
            ),
        }
    );
    if layer.refraction > 0.0 {
        let _ = writeln!(
            out,
            "  refraction {}: {}% of the shift follows the surface's direction on screen like a lens (a clean bulge), the rest is noise shimmer",
            fmt_num(layer.refraction),
            fmt_num((layer.refraction * 100.0).round())
        );
    }
    let defaults = param_values(project, &[]);
    let v = layer.values_at(&lookup(&defaults), 0.0);
    if v.tint.w > 0.0 {
        let _ = writeln!(
            out,
            "  tint: rgb × ({}, {}, {}) at {}% (multiplies what is behind)",
            fmt_num(v.tint.x),
            fmt_num(v.tint.y),
            fmt_num(v.tint.z),
            fmt_num((v.tint.w.min(1.0) * 100.0).round())
        );
    }
    if let Some(vis) = &layer.visible {
        let _ = writeln!(out, "  visible: only while {} is in [{}, {}], fading over {} outside", vis.param, fmt_num(vis.from), fmt_num(vis.to), fmt_num(vis.fade));
    }
    let _ = writeln!(out, "  at the defaults (age 0): {}", strength_text(v.strength));
    let ages = sample_ages(layer);
    if !ages.is_empty() {
        let label = if layer.is_one_shot() { "over one play" } else { "over its age" };
        let _ = writeln!(out, "  {} (default parameters):", label);
        for age in ages {
            let values = layer.values_at(&lookup(&defaults), layer.binding_age(age));
            let level = layer.envelope(age);
            let size = size_text(layer, values.scale).map(|s| format!("{}, ", s)).unwrap_or_default();
            let _ = writeln!(out, "    at {} s: {}{}", fmt_num(age), size, strength_text(values.strength * level));
        }
    }
    for name in layer.params_used() {
        let Some(p) = project.param(&name) else {
            let _ = writeln!(out, "  reads \"{}\", which is not a parameter", name);
            continue;
        };
        match p.kind() {
            ParamKind::Color => {
                let _ = writeln!(out, "  tint follows the Color parameter \"{}\"", name);
            }
            ParamKind::Float => {
                let at = |x: f32| {
                    let values = param_values(project, &[(name.clone(), ParamValue::Float(x))]);
                    let level = layer.visible.as_ref().filter(|v| v.param == name).map(|v| v.factor(x)).unwrap_or(1.0);
                    let on = match &layer.play {
                        PlayDef::While(param, t) if *param == name => x >= *t,
                        _ => true,
                    };
                    strength_text(layer.values_at(&lookup(&values), 0.0).strength * if on { level } else { 0.0 })
                };
                let _ = writeln!(out, "  at {}={} (min): {}", name, fmt_num(p.min), at(p.min));
                let _ = writeln!(out, "  at {}={} (middle): {}", name, fmt_num(p.middle()), at(p.middle()));
                let _ = writeln!(out, "  at {}={} (max): {}", name, fmt_num(p.max), at(p.max));
            }
        }
    }
    let _ = writeln!(
        out,
        "  camera: the game's camera needs Bevy's screen-space transmission (every Camera3d has ScreenSpaceTransmission {{ steps: 1 }} by default; keep steps above 0, or add omagari::distortion_camera(), which also adds Hdr); orthographic and perspective both work and the strength is relative to the view height, so it looks the same at any zoom"
    );
    let _ = writeln!(
        out,
        "  cost: one copy of the screen per frame and camera while any distortion is drawn; nothing while it is hidden or its strength is 0"
    );
}

/// The distortion layers' strength at each parameter set (the "Parameter
/// sets" section), at age 0.
pub fn distortion_sets_report(project: &ProjectV2, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    for (_, layer) in project.distortion_layers() {
        if only_effect.is_some_and(|o| o != layer.name) {
            continue;
        }
        let rows: Vec<String> = sets
            .iter()
            .map(|(_, set)| {
                let values = param_values(project, set);
                let x = |name: &str| values.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_float());
                let mut level = layer.visible.as_ref().and_then(|v| x(&v.param).map(|f| v.factor(f))).unwrap_or(1.0);
                if let PlayDef::While(param, t) = &layer.play
                    && !x(param).is_some_and(|v| v >= *t)
                {
                    level = 0.0;
                }
                strength_text(layer.values_at(&lookup(&values), 0.0).strength * level)
            })
            .collect();
        if rows.windows(2).all(|w| w[0] == w[1]) {
            let _ = writeln!(out, "  [{}] same in every set: {}", layer.name, rows.first().cloned().unwrap_or_default());
        } else {
            let _ = writeln!(out, "  [{}] changes with the parameters:", layer.name);
            for ((label, _), row) in sets.iter().zip(rows.iter()) {
                let _ = writeln!(out, "    {}: {}", label, row);
            }
        }
    }
}

/// When the distortions bend, in words ("Whole project").
pub fn distortions_lifecycle_text(project: &ProjectV2) -> Option<String> {
    let parts: Vec<String> = project
        .distortion_layers()
        .map(|(_, l)| match (&l.play, l.one_shot_duration()) {
            (PlayDef::OnStart, Some(d)) => format!("\"{}\" bends {} s after the start", l.name, fmt_num(d)),
            (PlayDef::OnTrigger(t), Some(d)) => format!("\"{}\" ripples {} s on \"{}\"", l.name, fmt_num(d), t),
            (PlayDef::Between { .. }, _) => format!("\"{}\" bends {}", l.name, between_when(project, &l.play)),
            (PlayDef::While(p, t), _) => format!("\"{}\" bends while {} ≥ {}", l.name, p, fmt_num(*t)),
            _ => format!("\"{}\" bends while the effect runs", l.name),
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!("distortion (opaque things behind it only): {}", parts.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::analysis::{analyze_project, check_project};

    fn has(list: &[String], needle: &str) -> bool {
        list.iter().any(|m| m.contains(needle))
    }

    fn project(layers: &str) -> ProjectV2 {
        let text = format!(
            r#"(version: 2,
                parameters: [(name: "heat", default: Float(1.0))],
                triggers: [(name: "slam")],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise))),
                         (id: "sprite", kind: Texture((path: "omagari://textures/glow1.png")))],
                sockets: [(name: "base", node: None, offset: (0.3, 1.0, 0.0)), (name: "tip", node: None, offset: (0.3, 2.0, 0.0))],
                layers: [{}],
            )"#,
            layers
        );
        ProjectV2::from_ron_str(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    const GOOD: &str = r#"Distortion((name: "haze", attach: Segment(from: "base", to: "tip"), source: Shape(shape: Capsule(radius: 0.12)),
            strength: Param("heat", [(0.0, 0.0), (1.0, 0.01)]),
            noise: (texture: (asset: "noise"), scale: 1.2, scroll: (0.0, 0.7)),
            tint: Const((1.08, 1.0, 0.92, 0.35)))),
        Distortion((name: "ripple", source: Shape(shape: Sphere(radius: 1.0), follow: Position), edge: Rim, refraction: 0.75, edge_softness: 0.35,
            scale: Age([(0.0, (0.3, 0.3, 0.3)), (0.6, (3.0, 3.0, 3.0))]), strength: Age([(0.0, 0.03), (0.6, 0.0)]),
            noise: (texture: (asset: "noise")), play: OnTrigger("slam"), duration: Some(0.6), fade: (in: 0.0, out: 0.1))),
        Light((name: "glow", intensity: Const(5000.0)))"#;

    #[test]
    fn good_distortions_pass_and_are_described() {
        let p = project(GOOD);
        let mut p = p;
        p.assets.retain(|a| a.id == "noise");
        let d = check_project(&p, &root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root(), 50, None);
        for needle in [
            "## [0] \"haze\" — distortion layer: a capsule of radius 0.12 m spanning the segment from socket \"base\"",
            "bends: what is behind it, opaque things only",
            "strength: curve over \"heat\": 0→0, 1→0.010 (fraction of the view height)",
            "at heat=0 (min): nothing (not drawn, no cost)",
            "at heat=1 (max): moves the picture by up to 0.010 of the view height (~11 px on a 1080 px view): a heat shimmer",
            "tint: rgb × (1.08, 1, 0.92) at 35%",
            "## [1] \"ripple\" — distortion layer: sphere, radius 1 m",
            "edge: Rim",
            "refraction 0.75",
            "over one play (default parameters):",
            "at 0 s: ~0.60 m across, moves the picture by up to 0.030 of the view height (~32 px on a 1080 px view): a strong ripple",
            "camera: the game's camera needs Bevy's screen-space transmission",
            "distortion (opaque things behind it only): \"haze\" bends while the effect runs, \"ripple\" ripples 0.60 s on \"slam\"",
            "trigger \"slam\": bursts ripple",
            "[haze] changes with the parameters:",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
    }

    #[test]
    fn distortion_problems_are_reported() {
        let bad = r#"Distortion((name: "bad", attach: Socket("nowhere"), play: OnTrigger("boom"), strength: Const(0.2),
                source: Shape(shape: Sphere(radius: 0.0)), noise: (texture: (asset: "missing")), edge_softness: 2.0)),
            Distortion((name: "b", strength: Const(0.0005), noise: (texture: (asset: "sprite")), noise_b: Some(()))),
            Distortion((name: "c", strength: Param("cold", [(0.0, 0.0), (1.0, 0.01)]), source: Surface(offset: -0.01), attach: Socket("base"))),
            Distortion((name: "d", strength: Const(0.01))),
            Particles((name: "sparks"))"#;
        let d = check_project(&project(bad), &root());
        for e in [
            "[bad] plays on trigger \"boom\", which is not a trigger of this package",
            "[bad] Sphere radius 0 must be above 0 metres",
            "[bad] noise texture \"missing\" is not in the assets list",
            "[c] strength reads \"cold\", which is not a parameter of this package",
        ] {
            assert!(has(&d.errors, e), "missing error {:?}\n{}", e, d.render());
        }
        for w in [
            "[bad] strength up to 0.20 of the view height (~216 px on a 1080 px view): the picture tears",
            "[bad] plays on a trigger but has no duration",
            "[bad] edge_softness 2 is outside 0..1",
            "[b] strength at most 0.001: under ~1 px",
            "[b] noise texture \"sprite\" wraps with Clamp",
            "[b] noise_b has no texture",
            "[c] surface offset -0.010 m is inside the model",
            "[d] no noise texture and no refraction: nothing moves the picture",
        ] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }
        for n in [
            "[bad] \"nowhere\" is not a package socket",
            "[c] a Surface source covers the target's own meshes: its attach is not used",
            "the distortion bends only opaque things (the floor, walls, models): this package's \"sparks\" is drawn after it",
        ] {
            assert!(has(&d.notes, n), "missing note {:?}\n{}", n, d.render());
        }
    }
}
