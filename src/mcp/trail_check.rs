//! Checks and descriptions of trail and afterimage layers (P5, F5b / F5c)
//! for `check_project` and `analyze_project`. They are not simulated (they
//! depend on how the game moves the sockets / the model): the description
//! gives the trail's time length, its width from the socket distance, its
//! fade and width along it, how emission starts and stops, and for
//! afterimages how many copies are on screen.

use std::fmt::Write;
use std::path::Path;

use bevy::prelude::*;
use omagari::format::*;

use super::analysis::{fmt_num, param_values, Diagnostics, ParamSet, PreviewModelNodes};
use super::surface_check::{check_material, color_text, describe_material, lookup, preview_target_model, MaterialOn};

/// Trails longer than this (seconds) are reported as long.
pub const LONG_TRAIL: f32 = 3.0;
/// More subdivisions than this cost vertices for no visible gain.
pub const MAX_USEFUL_SUBDIVISIONS: u32 = 16;
/// One frame at 60 fps: afterimages copy at most once per frame.
pub const FRAME: f32 = 1.0 / 60.0;
/// A typical sword tip speed during a swing (m/s), for the arc estimate.
pub const SWING_TIP_SPEED: f32 = 10.0;

fn check_socket(d: &mut Diagnostics, project: &ProjectV2, layer: &str, what: &str, socket: &str, model: Option<&PreviewModelNodes>) {
    if socket.trim().is_empty() {
        d.error(layer, format!("{} names no socket", what));
    } else if project.socket(socket).is_some() {
    } else if let Some(model) = model {
        if !model.names.iter().any(|n| n == socket) {
            d.error(
                layer,
                format!("{} \"{}\" is neither a package socket nor a node {}; that end would sit at the target root", what, socket, model.place),
            );
        }
    } else {
        let has_model = project.preview.as_ref().is_some_and(|p| p.model.is_some());
        d.note(
            layer,
            format!(
                "{} \"{}\" is not a package socket, so it must be a node name in the target's model{}",
                what,
                socket,
                if has_model { "" } else { " (no preview model to check against)" }
            ),
        );
    }
}

fn check_emit(d: &mut Diagnostics, project: &ProjectV2, layer: &str, emit: &EmitDef) {
    match emit {
        EmitDef::Always => {}
        EmitDef::Trigger { start, stop } => {
            for (what, name) in [("start", start), ("stop", stop)] {
                if name.trim().is_empty() {
                    d.error(layer, format!("emit Trigger has no {} trigger", what));
                } else if project.trigger(name).is_none() {
                    d.error(layer, format!("emit {} trigger \"{}\" is not a trigger of this package", what, name));
                }
            }
            if start == stop && !start.is_empty() {
                d.note(layer, format!("start and stop are the same trigger \"{}\": each firing switches emission on or off", start));
            }
        }
        EmitDef::While(param, _) => match project.param(param).map(|p| p.kind()) {
            None => d.error(layer, format!("emit While reads \"{}\", which is not a parameter of this package", param)),
            Some(ParamKind::Color) => d.error(layer, format!("emit While reads \"{}\", a Color parameter; it needs a Float one", param)),
            Some(ParamKind::Float) => {}
        },
    }
}

/// A curve over 0..1: sorted keys inside 0..1, values in `range`.
fn check_unit_curve(d: &mut Diagnostics, layer: &str, what: &str, keys: &[(f32, f32)], range: (f32, f32)) {
    if keys.windows(2).any(|w| w[1].0 < w[0].0) {
        d.error(layer, format!("{} keys must be sorted", what));
    }
    if keys.iter().any(|k| !(0.0..=1.0).contains(&k.0)) {
        d.warn(layer, format!("{} has keys outside 0..1 (the curve runs from 0 to 1)", what));
    }
    if keys.iter().any(|k| k.1 < range.0 || k.1 > range.1) {
        d.warn(layer, format!("{} has values outside {}..{}", what, fmt_num(range.0), fmt_num(range.1)));
    }
    if !keys.is_empty() && keys.iter().all(|k| k.1 <= 0.0) {
        d.warn(layer, format!("{} is 0 everywhere: nothing shows", what));
    }
}

fn check_fade(d: &mut Diagnostics, layer: &str, fade: &FadeDef) {
    if fade.fade_in < 0.0 || fade.fade_out < 0.0 {
        d.error(layer, "fade in / out must be 0 or more seconds");
    }
}

fn check_name(d: &mut Diagnostics, project: &ProjectV2, index: usize, name: &str, kind: &str) {
    if name.trim().is_empty() {
        d.errors.push(format!("a {} layer has an empty name", kind));
    }
    if project.layers.iter().enumerate().any(|(i, l)| i != index && l.name() == name) {
        d.error(name, "duplicate layer name; names must be unique");
    }
}

/// Every check of the trail and afterimage layers.
pub fn check_trail_layers(d: &mut Diagnostics, project: &ProjectV2, model: Option<&PreviewModelNodes>, assets_root: &Path) {
    for (index, layer) in project.trail_layers() {
        let name = layer.name.as_str();
        check_name(d, project, index, name, "trail");
        check_socket(d, project, name, "from", &layer.from, model);
        check_socket(d, project, name, "to", &layer.to, model);
        if layer.from == layer.to && !layer.from.is_empty() {
            d.error(name, format!("from and to are the same socket \"{}\": the trail has no width", layer.from));
        } else if let Some(w) = socket_distance(project, &layer.from, &layer.to) {
            if w < 0.01 {
                d.warn(name, format!("the sockets are {} m apart: the trail is a thin line", fmt_num(w)));
            }
        }
        if !(layer.length > 0.0) {
            d.error(name, format!("length {} must be above 0 seconds", fmt_num(layer.length)));
        } else if layer.length > LONG_TRAIL {
            d.warn(
                name,
                format!(
                    "length {} s is long for a trail (a sword swing: 0.15–0.4 s); samples are capped at {}, so a long trail gets coarse",
                    fmt_num(layer.length),
                    MAX_TRAIL_SAMPLES
                ),
            );
        }
        if layer.subdivisions == 0 {
            d.warn(name, "subdivisions 0 is used as 1 (straight lines between samples)");
        } else if layer.subdivisions > MAX_USEFUL_SUBDIVISIONS {
            d.warn(name, format!("subdivisions {} adds vertices without a visible gain; 4–8 keeps fast swings round", layer.subdivisions));
        }
        if layer.min_sample_distance < 0.0 {
            d.error(name, "min_sample_distance must be 0 or more metres");
        } else if layer.min_sample_distance > 0.5 {
            d.warn(name, format!("min_sample_distance {} m is coarse: samples come only every half metre", fmt_num(layer.min_sample_distance)));
        }
        check_emit(d, project, name, &layer.emit);
        check_unit_curve(d, name, "width_over_u", &layer.width_over_u, (0.0, 4.0));
        check_unit_curve(d, name, "fade_over_u", &layer.fade_over_u, (0.0, 1.0));
        check_fade(d, name, &layer.fade);
        check_material(d, project, name, &layer.material, layer.visible.as_ref());
        if let Some(fill) = &layer.material.fill {
            if !fill.axis.is_uv() {
                d.warn(name, format!("fill along {} on a trail: use U (along the trail's age) or V (from → to)", fill.axis.name()));
            }
        }
    }

    let preview_model = preview_target_model(project, assets_root);
    for (index, layer) in project.afterimage_layers() {
        let name = layer.name.as_str();
        check_name(d, project, index, name, "afterimage");
        if let GhostSourceDef::Meshes(names) = &layer.source {
            if names.is_empty() {
                d.warn(name, "source Meshes([]) copies nothing (use TargetModel for every mesh)");
            }
            for mesh in names {
                match &preview_model {
                    Some((path, info)) if !info.has_mesh_or_material(mesh) => d.error(
                        name,
                        format!("source mesh \"{}\" is not a mesh or mesh entity of the preview model {} (describe_model lists them)", mesh, path),
                    ),
                    Some(_) => {}
                    None => d.note(name, format!("source mesh \"{}\" must be a mesh name of the game's model (no preview model to check it)", mesh)),
                }
            }
        }
        if !(layer.interval > 0.0) {
            d.error(name, format!("interval {} must be above 0 seconds", fmt_num(layer.interval)));
        } else if layer.interval < FRAME * 0.99 {
            d.warn(
                name,
                format!("interval {} s is shorter than a frame (~{} s at 60 fps): at most one copy per frame is made", fmt_num(layer.interval), fmt_num(FRAME)),
            );
        }
        if !(layer.lifetime > 0.0) {
            d.error(name, format!("lifetime {} must be above 0 seconds", fmt_num(layer.lifetime)));
        } else if layer.interval > 0.0 && layer.lifetime < layer.interval {
            d.warn(
                name,
                format!(
                    "lifetime {} s is shorter than the interval {} s: copies blink one at a time instead of trailing",
                    fmt_num(layer.lifetime),
                    fmt_num(layer.interval)
                ),
            );
        }
        if layer.max_ghosts == 0 {
            d.warn(name, "max_ghosts 0 is used as 1");
        } else if layer.max_ghosts > MAX_GHOSTS_LIMIT {
            d.warn(name, format!("max_ghosts {} is capped at {}", layer.max_ghosts, MAX_GHOSTS_LIMIT));
        }
        if layer.interval > 0.0 && layer.lifetime > 0.0 {
            let made = (layer.lifetime / layer.interval).ceil() as u32;
            let cap = layer.ghost_cap() as u32;
            if made > cap {
                d.warn(
                    name,
                    format!(
                        "a copy every {} s that lives {} s makes ~{} at once, but max_ghosts is {}: the oldest vanish before they fade; raise max_ghosts or the interval",
                        fmt_num(layer.interval),
                        fmt_num(layer.lifetime),
                        made,
                        cap
                    ),
                );
            }
        }
        check_emit(d, project, name, &layer.emit);
        check_unit_curve(d, name, "fade_over_life", &layer.fade_over_life, (0.0, 1.0));
        check_fade(d, name, &layer.fade);
        check_material(d, project, name, &layer.material, layer.visible.as_ref());
        for (what, last) in layer.material.age_bindings() {
            if last > layer.lifetime + 1e-4 && layer.lifetime > 0.0 {
                d.warn(
                    name,
                    format!(
                        "{} curve over age has keys up to {} s, past the copies' lifetime {} s: they are gone before",
                        what,
                        fmt_num(last),
                        fmt_num(layer.lifetime)
                    ),
                );
            }
        }
    }
}

/// The distance between two sockets when it does not depend on the model:
/// both offsets from the target root, or from the same node.
pub fn socket_distance(project: &ProjectV2, a: &str, b: &str) -> Option<f32> {
    let (sa, sb) = (project.socket(a)?, project.socket(b)?);
    (sa.node == sb.node).then(|| sa.offset.distance(sb.offset))
}

fn socket_words(project: &ProjectV2, name: &str) -> String {
    match project.socket(name) {
        Some(s) => match &s.node {
            Some(node) => format!("socket \"{}\" (node \"{}\" + offset {:?})", name, node, s.offset.to_array()),
            None => format!("socket \"{}\" (target root + offset {:?})", name, s.offset.to_array()),
        },
        None => format!("node \"{}\" of the target's model", name),
    }
}

/// How emission starts and stops, in words.
pub fn emit_text(project: &ProjectV2, emit: &EmitDef, after: &str) -> String {
    match emit {
        EmitDef::Always => format!("Always: emits while the effect runs; after the game stops the effect {}", after),
        EmitDef::Trigger { start, stop } if start == stop => format!(
            "Trigger(\"{}\") toggles: each time the game fires it emission switches on or off; when it switches off {}",
            start, after
        ),
        EmitDef::Trigger { start, stop } => {
            let known = |t: &str| if project.trigger(t).is_some() { "" } else { " (NOT a trigger of this package)" };
            format!(
                "Trigger: starts on \"{}\"{} and stops on \"{}\"{} (the game fires them from its attack state, e.g. at the start and end of a swing); after the stop {}; nothing is drawn between swings",
                start,
                known(start),
                stop,
                known(stop),
                after
            )
        }
        EmitDef::While(p, t) => format!("While(\"{}\", {}): emits while {} ≥ {}; when it drops {}", p, fmt_num(*t), p, fmt_num(*t), after),
    }
}

fn curve_text(keys: &[(f32, f32)], at: &[f32], value: impl Fn(f32) -> f32) -> String {
    if keys.is_empty() {
        return "1 everywhere".into();
    }
    at.iter().map(|x| format!("{}: {}", fmt_num(*x), fmt_num(value(*x)))).collect::<Vec<_>>().join(", ")
}

fn look_line(m: &EffectMaterialDef, v: &EffectValues) -> String {
    let blend = match m.blend {
        EffectBlendDef::Blend => "Blend (covers what is behind by its opacity)",
        EffectBlendDef::Add => "Add (adds light, never darkens)",
    };
    format!("look: {}; colour {} × emissive {}; opacity {}", blend, color_text(v.color), fmt_num(v.emissive), fmt_num(v.opacity))
}

fn params_text(project: &ProjectV2, names: Vec<String>, m: &EffectMaterialDef, out: &mut String) {
    for name in names {
        let Some(p) = project.param(&name) else {
            let _ = writeln!(out, "  reads \"{}\", which is not a parameter", name);
            continue;
        };
        match p.kind() {
            ParamKind::Color => {
                let _ = writeln!(out, "  colour follows the Color parameter \"{}\"", name);
            }
            ParamKind::Float => {
                for (label, x) in [("min", p.min), ("max", p.max)] {
                    let values = param_values(project, &[(name.clone(), ParamValue::Float(x))]);
                    let v = m.evaluate(&lookup(&values));
                    let _ = writeln!(
                        out,
                        "  at {}={} ({}): colour {} × {}, opacity {}",
                        name,
                        fmt_num(x),
                        label,
                        color_text(v.color),
                        fmt_num(v.emissive),
                        fmt_num(v.opacity * v.color.w)
                    );
                }
            }
        }
    }
}

/// The description of a trail layer for `analyze_project`.
pub fn describe_trail_layer(project: &ProjectV2, index: usize, layer: &TrailLayerDef, out: &mut String) {
    let _ = writeln!(
        out,
        "\n## [{}] \"{}\" — trail layer: a ribbon from {} to {}",
        index,
        layer.name,
        socket_words(project, &layer.from),
        socket_words(project, &layer.to)
    );
    match socket_distance(project, &layer.from, &layer.to) {
        Some(w) => {
            let _ = writeln!(out, "  width: {} m (the sockets' distance, × the size fit; scaled along the trail by width_over_u)", fmt_num(w));
        }
        None => {
            let _ = writeln!(out, "  width: the distance between the two nodes in the model's pose (not known without the model)");
        }
    }
    let _ = writeln!(
        out,
        "  length: {} s in time — each point stays {} s; a blade tip moving {} m/s draws an arc ~{} m long; in world space (it stays where the blade was)",
        fmt_num(layer.length),
        fmt_num(layer.length),
        fmt_num(SWING_TIP_SPEED),
        fmt_num(SWING_TIP_SPEED * layer.length)
    );
    let after = format!("the trail ages out over {} s", fmt_num(layer.length));
    let _ = writeln!(out, "  emit: {}", emit_text(project, &layer.emit, &after));
    let _ = writeln!(
        out,
        "  smoothing: {} Catmull-Rom segments between samples; a sample whenever an end moved {} m (the newest point always follows the sockets)",
        layer.subdivisions.max(1),
        fmt_num(layer.min_sample_distance)
    );
    let at = [0.0, 0.5, 1.0];
    let _ = writeln!(out, "  along the trail (u: 0 newest → 1 oldest): width {}", curve_text(&layer.width_over_u, &at, |u| layer.width_at(u)));
    let _ = writeln!(out, "  along the trail: opacity {}", curve_text(&layer.fade_over_u, &at, |u| layer.fade_at(u)));
    let defaults = param_values(project, &[]);
    let v = layer.material.evaluate(&lookup(&defaults));
    let _ = writeln!(out, "  {}", look_line(&layer.material, &v));
    describe_material(project, &layer.material, &v, MaterialOn::Trail, out);
    if layer.material.reads_age() {
        let _ = writeln!(out, "  age curves read the seconds since emission last started");
    }
    if layer.fade.fade_in > 0.0 || layer.fade.fade_out > 0.0 {
        let _ = writeln!(out, "  lifecycle fade: in {} s, out {} s", fmt_num(layer.fade.fade_in), fmt_num(layer.fade.fade_out));
    }
    if layer.sort_bias != 0.0 {
        let _ = writeln!(out, "  draw order: sort bias {} m", fmt_num(layer.sort_bias));
    }
    if let Some(vis) = &layer.visible {
        let _ = writeln!(out, "  visible: only while {} is in [{}, {}]", vis.param, fmt_num(vis.from), fmt_num(vis.to));
    }
    params_text(project, layer.params_used(), &layer.material, out);
}

/// The description of an afterimage layer for `analyze_project`.
pub fn describe_afterimage_layer(project: &ProjectV2, index: usize, layer: &AfterimageLayerDef, out: &mut String) {
    let source = match &layer.source {
        GhostSourceDef::TargetModel => "every mesh of the target's own model".to_string(),
        GhostSourceDef::Meshes(names) => format!("the meshes {}", names.iter().map(|n| format!("\"{}\"", n)).collect::<Vec<_>>().join(", ")),
    };
    let _ = writeln!(out, "\n## [{}] \"{}\" — afterimage layer: fading copies of {}", index, layer.name, source);
    let _ = writeln!(
        out,
        "  a copy every {} s while emitting, each staying {} s: ~{} copies on screen (max_ghosts {}); copies stay where they were left in the world",
        fmt_num(layer.interval),
        fmt_num(layer.lifetime),
        layer.ghosts_on_screen(),
        layer.ghost_cap()
    );
    let _ = writeln!(
        out,
        "  opacity over a copy's life (0 = new, 1 = gone): {}",
        curve_text(&layer.fade_over_life, &[0.0, 0.5, 1.0], |x| unit_curve(&layer.fade_over_life, x).clamp(0.0, 1.0))
    );
    let _ = writeln!(
        out,
        "  skinned meshes are copied in their current pose (frozen joints); each copy costs one draw per mesh"
    );
    let after = format!("the last copies fade out over {} s", fmt_num(layer.lifetime));
    let _ = writeln!(out, "  emit: {}", emit_text(project, &layer.emit, &after));
    let defaults = param_values(project, &[]);
    let v = layer.material.evaluate(&lookup(&defaults));
    let _ = writeln!(out, "  {}", look_line(&layer.material, &v));
    describe_material(project, &layer.material, &v, MaterialOn::Body, out);
    if layer.material.reads_age() {
        let _ = writeln!(out, "  age curves read each copy's age in seconds");
    }
    if let Some(vis) = &layer.visible {
        let _ = writeln!(out, "  visible: only while {} is in [{}, {}]", vis.param, fmt_num(vis.from), fmt_num(vis.to));
    }
    params_text(project, layer.params_used(), &layer.material, out);
}

/// The trail and afterimage looks at each parameter set ("Parameter sets").
pub fn emitter_sets_report(project: &ProjectV2, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    let layers = project
        .trail_layers()
        .map(|(_, l)| (l.name.as_str(), &l.material))
        .chain(project.afterimage_layers().map(|(_, l)| (l.name.as_str(), &l.material)));
    for (name, material) in layers {
        if only_effect.is_some_and(|o| o != name) {
            continue;
        }
        let rows: Vec<String> = sets
            .iter()
            .map(|(_, set)| {
                let v = material.evaluate(&lookup(&param_values(project, set)));
                format!("colour {} × {}, opacity {}", color_text(v.color), fmt_num(v.emissive), fmt_num(v.opacity * v.color.w))
            })
            .collect();
        if rows.windows(2).all(|w| w[0] == w[1]) {
            let _ = writeln!(out, "  [{}] same in every set: {}", name, rows.first().cloned().unwrap_or_default());
        } else {
            let _ = writeln!(out, "  [{}] changes with the parameters:", name);
            for ((label, _), row) in sets.iter().zip(rows.iter()) {
                let _ = writeln!(out, "    {}: {}", label, row);
            }
        }
    }
}

/// How trails and afterimages run, for the "Whole project" section.
pub fn emitters_lifecycle_text(project: &ProjectV2) -> Option<String> {
    let parts: Vec<String> = project
        .trail_layers()
        .map(|(_, l)| (l.name.as_str(), &l.emit))
        .chain(project.afterimage_layers().map(|(_, l)| (l.name.as_str(), &l.emit)))
        .map(|(name, emit)| match emit {
            EmitDef::Always => format!("\"{}\" emits all the time", name),
            EmitDef::Trigger { start, stop } => format!("\"{}\" emits from \"{}\" to \"{}\"", name, start, stop),
            EmitDef::While(p, t) => format!("\"{}\" emits while {} ≥ {}", name, p, fmt_num(*t)),
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!("trails / afterimages run until the game stops the effect; {}", parts.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::analysis::{analyze_project, check_project};

    fn has(list: &[String], needle: &str) -> bool {
        list.iter().any(|m| m.contains(needle))
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    fn project(layer: &str) -> ProjectV2 {
        let text = format!(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5))],
                triggers: [(name: "swing_start"), (name: "swing_end")],
                sockets: [(name: "base", offset: (0.0, 0.1, 0.0)), (name: "tip", offset: (0.0, 1.0, 0.0))],
                layers: [{}],
            )"#,
            layer
        );
        ProjectV2::from_ron_str(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    const SLASH: &str = r#"Trail((name: "slash", from: "base", to: "tip", length: 0.2,
        emit: Trigger(start: "swing_start", stop: "swing_end"),
        width_over_u: [(0.0, 1.0), (1.0, 0.5)],
        material: (blend: Add, color: Const((2.0, 1.5, 1.0, 1.0)), opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)])),
    ))"#;

    #[test]
    fn a_good_trail_passes_and_is_described() {
        let p = project(SLASH);
        let d = check_project(&p, &root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root(), 50, None);
        for needle in [
            "## [0] \"slash\" — trail layer: a ribbon from socket \"base\" (target root + offset [0.0, 0.1, 0.0]) to socket \"tip\"",
            "width: 0.90 m (the sockets' distance",
            "length: 0.20 s in time",
            "draws an arc ~2 m long",
            "Trigger: starts on \"swing_start\" and stops on \"swing_end\"",
            "the trail ages out over 0.20 s",
            "width 0: 1, 0.50: 0.75, 1: 0.50",
            "opacity 0: 1, 0.50: 0.50, 1: 0",
            "at power=1 (max)",
            "trigger \"swing_start\": starts slash",
            "trigger \"swing_end\": stops slash",
            "\"slash\" emits from \"swing_start\" to \"swing_end\"",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
    }

    #[test]
    fn trail_problems_are_reported() {
        let bad = r#"Trail((name: "bad", from: "base", to: "base", length: 0.0, subdivisions: 40,
            emit: Trigger(start: "swing_start", stop: "nope"), fade_over_u: [(1.0, 0.0), (0.0, 1.0)],
            material: (fill: Some((axis: Y)))))"#;
        let d = check_project(&project(bad), &root());
        for e in [
            "[bad] from and to are the same socket \"base\"",
            "[bad] length 0 must be above 0 seconds",
            "[bad] emit stop trigger \"nope\" is not a trigger",
            "[bad] fade_over_u keys must be sorted",
        ] {
            assert!(has(&d.errors, e), "missing error {:?}\n{}", e, d.render());
        }
        for w in ["[bad] subdivisions 40", "[bad] fill along Y (up) on a trail"] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }
        let d = check_project(&project(r#"Trail((name: "n", from: "Hand", to: "tip"))"#), &root());
        assert!(has(&d.notes, "[n] from \"Hand\" is not a package socket"), "{}", d.render());
    }

    #[test]
    fn afterimages_are_checked_and_described() {
        let good = r#"Afterimage((name: "ghosts", interval: 0.05, lifetime: 0.3, max_ghosts: 8,
            emit: Trigger(start: "swing_start", stop: "swing_end"), material: (blend: Add)))"#;
        let p = project(&format!("{}, {}", SLASH, good));
        let d = check_project(&p, &root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root(), 50, None);
        for needle in [
            "## [1] \"ghosts\" — afterimage layer: fading copies of every mesh of the target's own model",
            "a copy every 0.050 s while emitting, each staying 0.30 s: ~6 copies on screen (max_ghosts 8)",
            "the last copies fade out over 0.30 s",
            "trigger \"swing_start\": starts slash, ghosts",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
        let bad = r#"Afterimage((name: "b", source: Meshes([]), interval: 0.01, lifetime: 1.0, max_ghosts: 4, emit: While("nope", 0.5)))"#;
        let d = check_project(&project(bad), &root());
        assert!(has(&d.errors, "[b] emit While reads \"nope\""), "{}", d.render());
        for w in ["[b] source Meshes([]) copies nothing", "[b] interval 0.010 s is shorter than a frame", "the oldest vanish before they fade"] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }
        let d = check_project(&project(r#"Afterimage((name: "z", interval: 0.0, lifetime: 0.0))"#), &root());
        assert!(has(&d.errors, "[z] interval 0 must be above 0") && has(&d.errors, "[z] lifetime 0 must be above 0"), "{}", d.render());
    }
}
