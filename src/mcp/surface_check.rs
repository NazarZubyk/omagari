//! Checks and descriptions of surface layers (P4) for `check_project` and
//! `analyze_project`, and of the effect material they share with shape
//! layers (`shape_check.rs`). Surface layers are not simulated: their look
//! is computed from the bindings at parameter values and described in words.

use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;
use omagari::format::*;

use super::analysis::{color_name, fmt_num, param_values, Diagnostics, ParamSet};
use crate::model_info::ModelInfo;

/// Opacity (colour alpha × opacity × the innermost shell, noise counted at
/// its average) from which a `Blend` overlay hides the model under it.
pub const OPAQUE_OPACITY: f32 = 0.95;

/// Shell offsets above this (metres) float visibly away from the body.
pub const MAX_SHELL_OFFSET: f32 = 0.1;

/// The model the preview instance is on (body, or the weapon with `target:
/// Weapon`), when its file is in the game assets folder.
pub fn preview_target_model(project: &ProjectV2, assets_root: &Path) -> Option<(String, Arc<ModelInfo>)> {
    let preview = project.preview.as_ref()?;
    let path = match preview.target {
        PreviewTargetModel::Weapon => preview.weapon.as_ref()?.model.clone(),
        PreviewTargetModel::Body => preview.model.clone()?,
    };
    let file = assets_root.join(&path);
    if !file.is_file() {
        return None;
    }
    ModelInfo::cached(&file).ok().map(|info| (path, info))
}

/// A lookup of parameter values for [`EffectMaterialDef::evaluate`].
pub(crate) fn lookup(values: &[(String, ParamValue)]) -> impl Fn(&str) -> Option<ParamValue> + '_ {
    move |name: &str| values.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
}

/// The opacity a `Blend` overlay's innermost shell covers the model with
/// (noise at its average: a noise of strength s lowers it by s / 2).
pub fn cover_opacity(layer: &SurfaceLayerDef, values: &EffectValues) -> f32 {
    let shell = layer
        .shells
        .iter()
        .min_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|s| s.opacity)
        .unwrap_or(0.0);
    let noise = |n: Option<NoiseValues>| n.map(|n| 1.0 - 0.5 * n.strength.clamp(0.0, 1.0)).unwrap_or(1.0);
    values.color.w * values.opacity * shell * noise(values.noise_a) * noise(values.noise_b)
}

/// The parameter sets the checks evaluate a layer at: the defaults, and every
/// Float parameter the layer reads at its min and max.
fn check_sets(project: &ProjectV2, layer: &SurfaceLayerDef) -> Vec<(String, Vec<(String, ParamValue)>)> {
    let mut sets = vec![("the defaults".to_string(), param_values(project, &[]))];
    for name in layer.params_used() {
        let Some(p) = project.param(&name).filter(|p| p.kind() == ParamKind::Float) else { continue };
        for (label, v) in [("min", p.min), ("max", p.max)] {
            sets.push((format!("{}={} ({})", name, fmt_num(v), label), param_values(project, &[(name.clone(), ParamValue::Float(v))])));
        }
    }
    sets
}

/// Every check of the surface layers.
pub fn check_surface_layers(d: &mut Diagnostics, project: &ProjectV2, assets_root: &Path) {
    let model = preview_target_model(project, assets_root);
    for (index, layer) in project.surface_layers() {
        let name = layer.name.as_str();
        if name.trim().is_empty() {
            d.errors.push("a surface layer has an empty name".into());
        }
        if project.layers.iter().enumerate().any(|(i, l)| i != index && l.name() == name) {
            d.error(name, "duplicate layer name; names must be unique");
        }

        // Targets.
        match &layer.targets {
            MeshTargetsDef::AllMeshes => {}
            targets => {
                if targets.names().is_empty() {
                    d.warn(name, "targets list is empty: the layer covers nothing (use AllMeshes for every mesh)");
                }
                for target in targets.names() {
                    match &model {
                        Some((path, info)) => {
                            if !info.has_mesh_or_material(target) {
                                d.error(
                                    name,
                                    format!(
                                        "target \"{}\" is not a mesh, mesh entity or material of the preview model {} (describe_model lists them)",
                                        target, path
                                    ),
                                );
                            }
                        }
                        None => d.note(
                            name,
                            format!("target \"{}\" must be a mesh or material name of the game's model (no preview model to check it)", target),
                        ),
                    }
                }
            }
        }

        // Shells and fades.
        if layer.shells.is_empty() {
            d.warn(name, "no shells: nothing is drawn (add (offset: 0.0) for a skin)");
        }
        for (i, shell) in layer.shells.iter().enumerate() {
            if shell.offset < 0.0 {
                d.warn(
                    name,
                    format!("shell {} offset {} m is inside the model: the model hides it", i + 1, fmt_num(shell.offset)),
                );
            } else if shell.offset > MAX_SHELL_OFFSET {
                d.warn(
                    name,
                    format!(
                        "shell {} offset {} m floats far from the body (shells are usually 0–0.05 m; a character is ~1.8 m)",
                        i + 1,
                        fmt_num(shell.offset)
                    ),
                );
            }
            if !(0.0..=1.0).contains(&shell.opacity) {
                d.warn(name, format!("shell {} opacity {} is outside 0..1", i + 1, fmt_num(shell.opacity)));
            }
        }
        if layer.fade.fade_in < 0.0 || layer.fade.fade_out < 0.0 {
            d.error(name, "fade in / out must be 0 or more seconds");
        }

        // Textures, parts and bindings.
        let m = &layer.material;
        check_material(d, project, name, m, layer.visible.as_ref());

        // Opaque Blend overlays hide the model.
        if m.blend == EffectBlendDef::Blend {
            for (label, values) in check_sets(project, layer) {
                let v = m.evaluate(&lookup(&values));
                let cover = cover_opacity(layer, &v);
                if cover >= OPAQUE_OPACITY {
                    d.warn(
                        name,
                        format!(
                            "at {} the Blend overlay covers the model with opacity {}: the model under it cannot be seen; lower the colour's opacity, add noise, or use Add",
                            label,
                            fmt_num(cover)
                        ),
                    );
                    break;
                }
            }
        }
    }
}

/// The checks of an effect material (surface and shape layers): texture
/// slots, parts that need others, curves and the parameters they read, and
/// the layer's `visible` range.
pub fn check_material(d: &mut Diagnostics, project: &ProjectV2, name: &str, m: &EffectMaterialDef, visible: Option<&VisibleRangeDef>) {
    let parts = [
        ("noise_a", m.noise_a.as_ref().map(|n| (&n.texture, &n.strength))),
        ("noise_b", m.noise_b.as_ref().map(|n| (&n.texture, &n.strength))),
        ("mask", m.mask.as_ref().map(|k| (&k.texture, &k.strength))),
    ];
    for (part, slot) in parts {
        let Some((slot, strength)) = slot else { continue };
        if slot.asset.trim().is_empty() {
            let strong = !matches!(strength, Binding::Const(s) if *s <= 0.0);
            if strong {
                d.warn(name, format!("{} has no texture but a strength above 0: that part is off; pick a texture (e.g. the built-in noise)", part));
            }
        } else if project.texture(&slot.asset).is_none() {
            d.error(name, format!("{} texture \"{}\" is not in the assets list", part, slot.asset));
        }
    }
    if m.dissolve.is_some() && m.noise_a.is_none() && m.noise_b.is_none() {
        d.warn(name, "dissolve breaks the surface up by the noise, but there is no noise A or B: nothing dissolves until the threshold reaches 1");
    }
    if let Some(rim) = &m.rim {
        if matches!(rim.power, Binding::Const(p) if p <= 0.0) {
            d.warn(name, "rim power must be above 0");
        }
    }
    if let Some(fill) = &m.fill {
        if let (Binding::Const(from), Binding::Const(to)) = (&fill.from, &fill.to) {
            if from > to {
                d.warn(name, format!("fill from {} is above to {}: nothing shows", fmt_num(*from), fmt_num(*to)));
            }
        }
    }
    check_bindings(d, project, name, m, visible);
}

fn check_bindings(d: &mut Diagnostics, project: &ProjectV2, name: &str, m: &EffectMaterialDef, visible: Option<&VisibleRangeDef>) {
    for (what, param) in m.binding_params() {
        match project.param(param).map(|p| p.kind()) {
            None => d.error(name, format!("{} reads \"{}\", which is not a parameter of this package", what, param)),
            Some(ParamKind::Color) if !EffectMaterialDef::is_color_binding(what) => {
                d.error(name, format!("{} reads \"{}\", a Color parameter; it needs a Float one", what, param))
            }
            _ => {}
        }
    }
    // Curves: keys present and sorted (a Color parameter needs none). An
    // age curve has the "parameter" "" here.
    fn keys<T>(b: &Binding<T>) -> Option<(Vec<f32>, &str)> {
        match b {
            Binding::Param(p, keys) => Some((keys.iter().map(|k| k.0).collect(), p.as_str())),
            Binding::Age(keys) => Some((keys.iter().map(|k| k.0).collect(), "")),
            Binding::Const(_) => None,
        }
    }
    let mut all: Vec<(&str, Vec<f32>, &str)> = Vec::new();
    macro_rules! add {
        ($what:expr, $b:expr) => {
            if let Some((xs, p)) = keys($b) {
                all.push(($what, xs, p));
            }
        };
    }
    add!("color", &m.color);
    add!("opacity", &m.opacity);
    add!("emissive", &m.emissive);
    for (label, n) in [("noise_a", &m.noise_a), ("noise_b", &m.noise_b)] {
        if let Some(n) = n {
            add!(label, &n.scale);
            add!(label, &n.scroll);
            add!(label, &n.strength);
            add!(label, &n.contrast);
        }
    }
    if let Some(r) = &m.rim {
        add!("rim.color", &r.color);
        add!("rim.power", &r.power);
        add!("rim.intensity", &r.intensity);
    }
    if let Some(p) = &m.pulse {
        add!("pulse.frequency", &p.frequency);
        add!("pulse.amount", &p.amount);
    }
    if let Some(f) = &m.flash {
        add!("flash", f);
    }
    if let Some(x) = &m.dissolve {
        add!("dissolve.threshold", &x.threshold);
        add!("dissolve.edge_width", &x.edge_width);
        add!("dissolve.edge_color", &x.edge_color);
    }
    if let Some(f) = &m.fill {
        add!("fill.from", &f.from);
        add!("fill.to", &f.to);
        add!("fill.softness", &f.softness);
    }
    if let Some(k) = &m.mask {
        add!("mask.scale", &k.scale);
        add!("mask.strength", &k.strength);
    }
    for (what, xs, param) in all {
        if param.is_empty() {
            if xs.is_empty() {
                d.error(name, format!("{} curve over age has no keys", what));
            }
            if xs.windows(2).any(|w| w[1] < w[0]) {
                d.error(name, format!("{} curve keys must be sorted by age", what));
            }
            if xs.iter().any(|x| *x < 0.0) {
                d.warn(name, format!("{} curve over age has keys below 0 s; the age starts at 0", what));
            }
            continue;
        }
        let is_color_param = project.param(param).is_some_and(|p| p.kind() == ParamKind::Color);
        if is_color_param {
            if !xs.is_empty() {
                d.warn(name, format!("{} uses the Color parameter \"{}\" directly; its keys are ignored", what, param));
            }
            continue;
        }
        if xs.is_empty() {
            d.error(name, format!("{} curve over \"{}\" has no keys", what, param));
        }
        if xs.windows(2).any(|w| w[1] < w[0]) {
            d.error(name, format!("{} curve keys must be sorted by parameter value", what));
        }
    }
    if let Some(visible) = visible {
        match project.param(&visible.param).map(|p| p.kind()) {
            None => d.error(name, format!("visible reads \"{}\", which is not a parameter of this package", visible.param)),
            Some(ParamKind::Color) => {
                d.error(name, format!("visible reads \"{}\", a Color parameter; it needs a Float one", visible.param))
            }
            Some(ParamKind::Float) => {}
        }
        if visible.from > visible.to {
            d.error(name, format!("visible from {} is above to {}", visible.from, visible.to));
        }
        if visible.fade < 0.0 {
            d.error(name, format!("visible fade {} must be 0 or more", visible.fade));
        }
    }
}

pub(crate) fn color_text(c: Vec4) -> String {
    format!("{} a{}", color_name(c.truncate()), fmt_num(c.w))
}

/// What a material is drawn on, for its description.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaterialOn {
    /// A surface layer's copies of the model's meshes.
    Body,
    /// A shape layer's mesh.
    Shape,
}

fn mapping_text(mapping: NoiseMappingDef, scale: f32, on: MaterialOn) -> String {
    match (mapping, on) {
        (NoiseMappingDef::Triplanar, MaterialOn::Body) => {
            format!("triplanar in the rest pose (sticks to the body), {} repeats per metre", fmt_num(scale))
        }
        (NoiseMappingDef::Triplanar, MaterialOn::Shape) => format!(
            "triplanar in the shape's space, {} repeats per metre (turns with the shape; a growing shape grows through the pattern)",
            fmt_num(scale)
        ),
        (NoiseMappingDef::Uv, MaterialOn::Body) => format!("by the mesh's UVs, {} repeats per UV square", fmt_num(scale)),
        (NoiseMappingDef::Uv, MaterialOn::Shape) => format!(
            "by the shape's UVs (u around / along, v up / across), {} repeats per UV square (sticks to the shape as it grows)",
            fmt_num(scale)
        ),
    }
}

fn noise_text(project: &ProjectV2, def: &NoiseDef, v: &NoiseValues, on: MaterialOn) -> String {
    let speed = v.scroll.length();
    let scroll = if speed <= 1e-6 {
        "still".to_string()
    } else {
        let metres = match (v.mapping, on) {
            (NoiseMappingDef::Triplanar, MaterialOn::Body) if v.scale > 0.0 => {
                format!(" ≈ {} m/s on the body", fmt_num(speed / v.scale))
            }
            (NoiseMappingDef::Triplanar, MaterialOn::Shape) if v.scale > 0.0 => {
                format!(" ≈ {} m/s", fmt_num(speed / v.scale))
            }
            _ => String::new(),
        };
        format!("scrolls ({}, {}) squares/s{}", fmt_num(v.scroll.x), fmt_num(v.scroll.y), metres)
    };
    let texture = match project.texture(&def.texture.asset) {
        Some(t) => format!("\"{}\" ({}) {}", def.texture.asset, t.path, def.texture.channel.name()),
        None if def.texture.asset.is_empty() => "NO TEXTURE (off)".to_string(),
        None => format!("\"{}\" MISSING (off)", def.texture.asset),
    };
    let contrast = if (v.contrast - 1.0).abs() > 1e-4 { format!(", contrast {}", fmt_num(v.contrast)) } else { String::new() };
    format!("{}; {}; {}; strength {}{}", texture, mapping_text(v.mapping, v.scale, on), scroll, fmt_num(v.strength), contrast)
}

/// The material's optional parts in words (noise, rim, pulse, flash,
/// dissolve, fill, mask), one line each, at the values `v`.
pub fn describe_material(project: &ProjectV2, m: &EffectMaterialDef, v: &EffectValues, on: MaterialOn, out: &mut String) {
    for (label, def, value) in [("noise A", &m.noise_a, v.noise_a), ("noise B", &m.noise_b, v.noise_b)] {
        if let (Some(def), Some(value)) = (def, value) {
            let _ = writeln!(out, "  {}: {}", label, noise_text(project, def, &value, on));
        }
    }
    if let Some((c, p, i)) = v.rim {
        let width = if p < 1.5 { "wide, soft" } else if p < 3.5 { "medium" } else { "thin" };
        let _ = writeln!(out, "  rim: {} × {} on the edges facing away from the camera ({} rim, power {})", color_text(c), fmt_num(i), width, fmt_num(p));
    }
    if let Some((f, a)) = v.pulse {
        let _ = writeln!(
            out,
            "  pulse: {} per second; opacity breathes between {} and 1 of its value",
            fmt_num(f),
            fmt_num((1.0 - a).clamp(0.0, 1.0))
        );
    }
    if let Some(flash) = &m.flash {
        let source = match flash.param() {
            Some(p) => {
                let pulses: Vec<String> = project
                    .triggers
                    .iter()
                    .flat_map(|t| t.pulses.iter().filter(|q| q.param == p).map(move |q| (t, q)))
                    .map(|(t, q)| format!("trigger \"{}\" pulses it to {} for {} s", t.name, fmt_num(q.to), fmt_num(q.decay)))
                    .collect();
                format!("follows \"{}\"{}", p, if pulses.is_empty() { String::new() } else { format!(" ({})", pulses.join("; ")) })
            }
            None if flash.reads_age() => format!("over the layer's age ({} at age 0)", fmt_num(v.flash)),
            None => format!("constant {}", fmt_num(v.flash)),
        };
        let _ = writeln!(out, "  flash: {} (1 = three times as bright, half-way to opaque)", source);
    }
    if let Some((t, w, c)) = v.dissolve {
        let _ = writeln!(out, "  dissolve: threshold {} (0 whole, 1 gone), edge {} wide, edge colour {}", fmt_num(t), fmt_num(w), color_text(c));
    }
    if let Some((axis, from, to, soft)) = v.fill {
        let what = match on {
            MaterialOn::Body => "the model",
            MaterialOn::Shape => "the shape",
        };
        let ends = if axis.is_uv() { "0..1 of the texture coordinate" } else { "0 = low end, 1 = high end" };
        let _ = writeln!(
            out,
            "  fill: shows the band {}..{} of {} along {} ({}), soft edges {}",
            fmt_num(from),
            fmt_num(to),
            what,
            axis.name(),
            ends,
            fmt_num(soft)
        );
    }
    if let (Some(def), Some((mapping, channel, scale, strength))) = (&m.mask, v.mask) {
        let _ = writeln!(
            out,
            "  mask: \"{}\" {}; {}; strength {}",
            def.texture.asset,
            channel.name(),
            mapping_text(mapping, scale, on),
            fmt_num(strength)
        );
    }
}

/// One line of a layer's look at a parameter set, for comparisons.
pub fn surface_summary(layer: &SurfaceLayerDef, values: &EffectValues) -> String {
    let mut parts = vec![
        format!("colour {}", color_text(values.color * Vec4::new(values.emissive, values.emissive, values.emissive, 1.0))),
        format!("opacity {}", fmt_num(values.opacity)),
        format!("covers at ~{}", fmt_num(cover_opacity(layer, values))),
    ];
    if let Some((c, p, i)) = values.rim {
        parts.push(format!("rim {} ×{} power {}", color_name(c.truncate()), fmt_num(i), fmt_num(p)));
    }
    if let Some(n) = values.noise_a {
        parts.push(format!("noise A strength {}", fmt_num(n.strength)));
    }
    if let Some((f, a)) = values.pulse {
        parts.push(format!("pulse {}/s amount {}", fmt_num(f), fmt_num(a)));
    }
    if values.flash > 0.0 {
        parts.push(format!("flash {}", fmt_num(values.flash)));
    }
    if let Some((t, _, _)) = values.dissolve {
        parts.push(format!("dissolve {}", fmt_num(t)));
    }
    if let Some((axis, from, to, _)) = values.fill {
        parts.push(format!("fill {} {}..{}", axis.name(), fmt_num(from), fmt_num(to)));
    }
    parts.join(", ")
}

/// The description of a surface layer for `analyze_project`.
pub fn describe_surface_layer(project: &ProjectV2, index: usize, layer: &SurfaceLayerDef, out: &mut String) {
    let targets = match &layer.targets {
        MeshTargetsDef::AllMeshes => "every mesh of the target's own model".to_string(),
        MeshTargetsDef::Meshes(n) => format!("the meshes {}", n.join(", ")),
        MeshTargetsDef::Materials(n) => format!("the meshes with materials {}", n.join(", ")),
    };
    let _ = writeln!(
        out,
        "\n## [{}] \"{}\" — surface layer over {} (copies of the meshes; the model is not changed; a held weapon is not covered)",
        index, layer.name, targets
    );
    let defaults = param_values(project, &[]);
    let v = layer.material.evaluate(&lookup(&defaults));
    let m = &layer.material;
    let blend = match m.blend {
        EffectBlendDef::Blend => "Blend (covers the model by its opacity)",
        EffectBlendDef::Add => "Add (adds light, never darkens)",
    };
    let _ = writeln!(
        out,
        "  look: {}; colour {} × emissive {}; opacity {}; covers the skin at ~{}",
        blend,
        color_text(v.color),
        fmt_num(v.emissive),
        fmt_num(v.opacity),
        fmt_num(cover_opacity(layer, &v))
    );
    describe_material(project, m, &v, MaterialOn::Body, out);
    if !m.age_bindings().is_empty() {
        let ages: Vec<String> = m.age_bindings().iter().map(|(w, last)| format!("{} (keys to {} s)", w, fmt_num(*last))).collect();
        let _ = writeln!(out, "  over its age (seconds since the effect started; a restart starts it again): {}", ages.join(", "));
    }
    let shells: Vec<String> = layer
        .shells
        .iter()
        .map(|s| format!("{} cm (opacity {})", fmt_num(s.offset * 100.0), fmt_num(s.opacity)))
        .collect();
    let thickness = layer.shells.iter().map(|s| s.offset).fold(0.0f32, f32::max);
    let _ = writeln!(
        out,
        "  shells: {} — {}{}",
        layer.shells.len(),
        shells.join(", "),
        if layer.shells.len() > 1 { format!("; ~{} cm of glow around the silhouette", fmt_num(thickness * 100.0)) } else { String::new() }
    );
    let _ = writeln!(
        out,
        "  fade: in {} s when the effect starts, out {} s when the game stops it (the stop waits for it)",
        fmt_num(layer.fade.fade_in),
        fmt_num(layer.fade.fade_out)
    );
    if let Some(vis) = &layer.visible {
        let _ = writeln!(
            out,
            "  visible: only while {} is in [{}, {}], fading over {} outside",
            vis.param,
            fmt_num(vis.from),
            fmt_num(vis.to),
            fmt_num(vis.fade)
        );
    }
    // Each Float parameter it reads, at its min and max.
    for name in layer.params_used() {
        let Some(p) = project.param(&name) else {
            let _ = writeln!(out, "  reads \"{}\", which is not a parameter", name);
            continue;
        };
        match p.kind() {
            ParamKind::Color => {
                let _ = writeln!(out, "  colour follows the Color parameter \"{}\" (default {})", name, color_text(p.default.as_color().unwrap_or(Vec4::ONE)));
            }
            ParamKind::Float => {
                let at = |x: f32| {
                    let values = param_values(project, &[(name.clone(), ParamValue::Float(x))]);
                    surface_summary(layer, &layer.material.evaluate(&lookup(&values)))
                };
                let _ = writeln!(out, "  at {}={} (min): {}", name, fmt_num(p.min), at(p.min));
                let _ = writeln!(out, "  at {}={} (max): {}", name, fmt_num(p.max), at(p.max));
            }
        }
    }
}

/// The surface layers' look at each parameter set (the "Parameter sets"
/// section).
pub fn surface_sets_report(project: &ProjectV2, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    for (_, layer) in project.surface_layers() {
        if only_effect.is_some_and(|o| o != layer.name) {
            continue;
        }
        let rows: Vec<String> = sets
            .iter()
            .map(|(_, set)| {
                let values = param_values(project, set);
                surface_summary(layer, &layer.material.evaluate(&lookup(&values)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::analysis::{analyze_project, check_project};

    fn has(list: &[String], needle: &str) -> bool {
        list.iter().any(|m| m.contains(needle))
    }

    fn project(layer: &str) -> ProjectV2 {
        let text = format!(
            r#"(version: 2,
                parameters: [(name: "power", default: Float(0.5)), (name: "tint", default: Color((1.0, 0.0, 0.0, 1.0)))],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [Surface(({}))],
            )"#,
            layer
        );
        ProjectV2::from_ron_str(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    const GOOD: &str = r#"name: "skin",
        material: (
            blend: Add,
            color: Param("tint", []),
            opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]),
            noise_a: Some((texture: (asset: "noise"), scale: Const(3.0), scroll: Const((0.0, 0.3)), strength: Const(0.8))),
            rim: Some((power: Const(2.0), intensity: Const(1.5))),
        ),
        shells: [(offset: 0.0), (offset: 0.015, opacity: 0.6), (offset: 0.03, opacity: 0.3)],"#;

    #[test]
    fn a_good_surface_layer_passes_and_is_described() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        let p = project(GOOD);
        let d = check_project(&p, &root);
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root, 50, None);
        for needle in [
            "## [0] \"skin\" — surface layer over every mesh of the target's own model",
            "look: Add (adds light, never darkens)",
            "triplanar in the rest pose (sticks to the body), 3 repeats per metre; scrolls (0, 0.30) squares/s ≈ 0.10 m/s",
            "rim: white a1 × 1.50",
            "shells: 3 — 0 cm (opacity 1), 1.50 cm (opacity 0.60), 3 cm (opacity 0.30); ~3 cm of glow around the silhouette",
            "colour follows the Color parameter \"tint\"",
            "at power=0 (min): colour red a1, opacity 0",
            "at power=1 (max): colour red a1, opacity 1",
            "[skin] changes with the parameters:",
            "no particle layers",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
    }

    #[test]
    fn surface_problems_are_reported() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        // An opaque Blend skin hides the model; Add never does.
        let opaque = r#"name: "paint", material: (blend: Blend, color: Const((1.0, 0.0, 0.0, 1.0)))"#;
        let d = check_project(&project(opaque), &root);
        assert!(has(&d.warnings, "[paint] at the defaults the Blend overlay covers the model with opacity 1"), "{}", d.render());
        let d = check_project(&project(&opaque.replace("Blend", "Add")), &root);
        assert!(!has(&d.warnings, "covers the model"), "{}", d.render());
        // ... and at a parameter's max.
        let rising = r#"name: "paint", material: (blend: Blend, color: Const((1.0, 0.0, 0.0, 1.0)), opacity: Param("power", [(0.0, 0.2), (1.0, 1.0)]))"#;
        let d = check_project(&project(rising), &root);
        assert!(has(&d.warnings, "at power=1 (max) the Blend overlay covers the model"), "{}", d.render());

        // Shells, textures, bindings.
        let bad = r#"name: "bad",
            material: (
                blend: Add,
                opacity: Param("tint", [(0.0, 1.0)]),
                emissive: Param("nope", [(0.0, 1.0)]),
                noise_a: Some((texture: (asset: ""), strength: Const(0.5))),
                noise_b: Some((texture: (asset: "missing"))),
                rim: Some((color: Param("power", []), power: Const(0.0))),
            ),
            shells: [(offset: -0.01), (offset: 0.3, opacity: 1.5)],
            fade: (in: -1.0),
            visible: Some((param: "tint", from: 1.0, to: 0.0)),"#;
        let d = check_project(&project(bad), &root);
        for e in [
            "[bad] opacity reads \"tint\", a Color parameter; it needs a Float one",
            "[bad] emissive reads \"nope\", which is not a parameter of this package",
            "[bad] noise_b texture \"missing\" is not in the assets list",
            "[bad] rim.color curve over \"power\" has no keys",
            "[bad] fade in / out must be 0 or more seconds",
            "[bad] visible reads \"tint\", a Color parameter",
            "[bad] visible from 1 is above to 0",
        ] {
            assert!(has(&d.errors, e), "missing error {:?}\n{}", e, d.render());
        }
        for w in [
            "[bad] noise_a has no texture but a strength above 0",
            "[bad] shell 1 offset -0.010 m is inside the model",
            "[bad] shell 2 offset 0.30 m floats far from the body",
            "[bad] shell 2 opacity 1.50 is outside 0..1",
            "[bad] rim power must be above 0",
        ] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }

        let empty = r#"name: "none", targets: Meshes([]), shells: [], material: (blend: Add, dissolve: Some(()))"#;
        let d = check_project(&project(empty), &root);
        assert!(has(&d.warnings, "targets list is empty"), "{}", d.render());
        assert!(has(&d.warnings, "no shells: nothing is drawn"), "{}", d.render());
        assert!(has(&d.warnings, "dissolve breaks the surface up by the noise, but there is no noise"), "{}", d.render());

        // Layer names are unique across kinds.
        let mut p = project(GOOD);
        p.layers.push(LayerDef::Particles(ParticlesLayerDef { name: "skin".into(), ..Default::default() }));
        assert!(has(&check_project(&p, &root).errors, "[skin] duplicate layer name"));
    }

    #[test]
    fn targets_are_checked_against_the_preview_model() {
        let dir = crate::asset_library::tests::temp_dir("surface_targets");
        crate::model_info::tests::write_fixture(&dir.join("hero.gltf"), None);
        let layer = r#"name: "skin", targets: Meshes(["Body", "Body.Skin", "Cape"]), material: (blend: Add)"#;
        let mut p = project(layer);
        p.assets.clear();
        let d = check_project(&p, &dir);
        assert!(has(&d.notes, "target \"Cape\" must be a mesh or material name of the game's model"), "{}", d.render());
        p.preview = Some(PreviewDef { stage: PreviewStage::Model, model: Some("hero.gltf".into()), ..Default::default() });
        let d = check_project(&p, &dir);
        assert!(has(&d.errors, "[skin] target \"Cape\" is not a mesh, mesh entity or material of the preview model hero.gltf"), "{}", d.render());
        assert!(!has(&d.errors, "\"Body\""), "{}", d.render());
        let p2 = {
            let mut p2 = p.clone();
            p2.layers[0].as_surface_mut().unwrap().targets = MeshTargetsDef::Materials(vec!["Metal".into(), "Skin".into()]);
            p2
        };
        let d = check_project(&p2, &dir);
        assert!(d.errors.is_empty() && !has(&d.notes, "target"), "{}", d.render());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
