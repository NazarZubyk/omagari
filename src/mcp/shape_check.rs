//! Checks and descriptions of shape layers (P4, F4b) for `check_project` and
//! `analyze_project`. Shapes are not simulated: their size, opacity and look
//! are worked out from the bindings at parameter values and over their age,
//! and described in words.

use std::fmt::Write;

use bevy::prelude::*;
use omagari::format::*;

use super::analysis::{fmt_num, param_values, Diagnostics, ParamSet, PreviewModelNodes};
use super::surface_check::{check_material, color_text, describe_material, lookup, MaterialOn, OPAQUE_OPACITY};

/// Every value a `Binding<Vec3>` can take (the constant, or every key).
fn scale_values(b: &Binding<Vec3>) -> Vec<Vec3> {
    match b {
        Binding::Const(v) => vec![*v],
        Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
    }
}

/// The ages a description samples: over the one-shot / repeat duration, or
/// up to the last `Age` key.
fn sample_ages(layer: &ShapeLayerDef) -> Vec<f32> {
    let span = match layer.duration {
        Some(d) if d > 0.0 => Some(d),
        _ => layer.age_bindings().iter().map(|(_, last)| *last).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b)))),
    };
    let Some(span) = span.filter(|s| *s > 0.0) else { return Vec::new() };
    let mut ages: Vec<f32> = [0.0, 0.25, 0.5, 0.75].iter().map(|f| f * span).collect();
    // Just before the end (a one-shot is gone at its duration).
    ages.push(if layer.is_one_shot() { span * 0.98 } else { span });
    ages
}

/// Opacity of the shape at its base: colour alpha × opacity × noise at its
/// average (as for surfaces), × the one-shot envelope.
fn shape_opacity(values: &EffectValues, envelope: f32) -> f32 {
    let noise = |n: Option<NoiseValues>| n.map(|n| 1.0 - 0.5 * n.strength.clamp(0.0, 1.0)).unwrap_or(1.0);
    values.color.w * values.opacity * noise(values.noise_a) * noise(values.noise_b) * envelope
}

/// The largest extent of the shape at `scale` (metres, before the fit).
fn extent(shape: &ShapeDef, scale: Vec3) -> f32 {
    let (lo, hi) = shape.bounds();
    ((hi - lo) * scale.abs()).max_element()
}

/// One line of a shape's look at a parameter set / age.
fn shape_summary(layer: &ShapeLayerDef, values: &EffectValues, scale: Vec3, envelope: f32) -> String {
    let mut parts = vec![
        format!("~{} m across (scale {})", fmt_num(extent(&layer.shape, scale)), scale_text(scale)),
        format!("colour {}", color_text(values.color * Vec4::new(values.emissive, values.emissive, values.emissive, 1.0))),
        format!("opacity ~{}", fmt_num(shape_opacity(values, envelope))),
    ];
    if let Some((_, power, intensity)) = values.rim {
        parts.push(format!("rim ×{} power {}", fmt_num(intensity), fmt_num(power)));
    }
    if let Some((t, _, _)) = values.dissolve {
        parts.push(format!("dissolve {}", fmt_num(t)));
    }
    if let Some((axis, from, to, _)) = values.fill {
        parts.push(format!("fill {} {}..{}", axis.name(), fmt_num(from), fmt_num(to)));
    }
    parts.join(", ")
}

fn scale_text(s: Vec3) -> String {
    if (s.x - s.y).abs() < 1e-4 && (s.x - s.z).abs() < 1e-4 {
        fmt_num(s.x)
    } else {
        format!("({}, {}, {})", fmt_num(s.x), fmt_num(s.y), fmt_num(s.z))
    }
}

/// Every check of the shape layers.
pub fn check_shape_layers(d: &mut Diagnostics, project: &ProjectV2, model: Option<&PreviewModelNodes>) {
    for (index, layer) in project.shape_layers() {
        let name = layer.name.as_str();
        if name.trim().is_empty() {
            d.errors.push("a shape layer has an empty name".into());
        }
        if project.layers.iter().enumerate().any(|(i, l)| i != index && l.name() == name) {
            d.error(name, "duplicate layer name; names must be unique");
        }

        // Size.
        for (what, value) in layer.shape.dimensions() {
            if !(value > 0.0) {
                d.error(name, format!("{} {} {} must be above 0 (metres{}): nothing is drawn", layer.shape.kind_name(), what, fmt_num(value), if what == "angle_deg" { "; degrees for the angle" } else { "" }));
            }
        }
        let scales = scale_values(&layer.scale);
        if scales.iter().any(|s| s.min_element() < 0.0) {
            d.warn(name, "scale has a value below 0: the shape turns inside out (use rotation to flip it)");
        }
        let biggest = scales.iter().map(|s| extent(&layer.shape, *s)).fold(0.0f32, f32::max);
        if biggest > HUGE_SHAPE {
            d.warn(
                name,
                format!(
                    "the shape gets ~{} m across (size × scale): huge for a character effect (~1.8 m tall; the game camera shows ~16 m); check the units",
                    fmt_num(biggest)
                ),
            );
        }
        if let ShapeDef::Ring { inner, outer } = layer.shape {
            if inner < 0.0 {
                d.warn(name, format!("ring inner radius {} is below 0", fmt_num(inner)));
            } else if inner >= outer {
                d.error(name, format!("ring inner radius {} must be below outer {}", fmt_num(inner), fmt_num(outer)));
            }
        }
        if let ShapeDef::Arc { radius, width, angle_deg, taper } = layer.shape {
            if width > 2.0 * radius && radius > 0.0 {
                d.warn(name, format!("arc width {} is more than twice its radius {}: the inner edge folds into the centre", fmt_num(width), fmt_num(radius)));
            }
            if angle_deg > 360.0 {
                d.warn(name, format!("arc angle {}° is more than a full turn: it overlaps itself", fmt_num(angle_deg)));
            }
            if !(0.0..=1.0).contains(&taper) {
                d.warn(name, format!("arc taper {} is outside 0..1 (0 = even width, 1 = pointed ends)", fmt_num(taper)));
            }
        }
        if layer.rotation_axis.length_squared() < 1e-8 && !matches!(layer.rotation_speed, Binding::Const(s) if s == 0.0) {
            d.warn(name, "rotation_axis is (0, 0, 0): the shape spins around Y instead");
        }
        if layer.shape.faces_camera() && layer.transform.rotation_deg != Vec3::ZERO {
            d.note(name, "a quad facing the camera ignores the rotation");
        }

        // Attach.
        check_attach_sockets(d, project, name, &layer.attach, model, "the shape");

        // Play, duration and age.
        match &layer.play {
            PlayDef::OnTrigger(trigger) => {
                if project.trigger(trigger).is_none() {
                    d.error(name, format!("plays on trigger \"{}\", which is not a trigger of this package", trigger));
                }
                if layer.duration.is_none() {
                    d.warn(
                        name,
                        "plays on a trigger but has no duration: after the first trigger it stays until the effect stops; set duration (seconds) for a one-shot",
                    );
                }
            }
            PlayDef::While(param, _) => match project.param(param).map(|p| p.kind()) {
                None => d.error(name, format!("play While reads \"{}\", which is not a parameter of this package", param)),
                Some(ParamKind::Color) => d.error(name, format!("play While reads \"{}\", a Color parameter; it needs a Float one", param)),
                Some(ParamKind::Float) => {}
            },
            PlayDef::Between { .. } => check_between(d, project, name, &layer.play),
            PlayDef::OnStart if layer.duration.is_none() => d.note(
                name,
                "OnStart without a duration plays like Loop (until the effect stops); set duration for a one-shot",
            ),
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
                            "fade in {} + out {} s is longer than the duration {} s: the shape never reaches full opacity",
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
                                if layer.is_one_shot() { "the shape is gone before they are reached" } else { "the age starts over before they are reached" }
                            ),
                        );
                    }
                }
            }
        } else if matches!(layer.play, PlayDef::Loop | PlayDef::While(..) | PlayDef::Between { .. }) && !layer.age_bindings().is_empty() {
            d.note(
                name,
                "curves over age run once from the start and then hold their last key; give the shape a duration to repeat them",
            );
        }
        if layer.fade.fade_in < 0.0 || layer.fade.fade_out < 0.0 {
            d.error(name, "fade in / out must be 0 or more seconds");
        }

        // Shape-specific bindings.
        let spin_keys = layer.rotation_speed.keys().map(|k| k.iter().map(|k| k.0).collect::<Vec<_>>());
        check_keys(d, name, "rotation_speed", spin_keys, layer.rotation_speed.param(), project);
        let scale_keys = layer.scale.keys().map(|k| k.iter().map(|k| k.0).collect::<Vec<_>>());
        check_keys(d, name, "scale", scale_keys, layer.scale.param(), project);

        // Material.
        let m = &layer.material;
        check_material(d, project, name, m, layer.visible.as_ref());
        if let Some(fill) = &m.fill {
            if layer.shape.is_flat() && matches!(fill.axis, AxisDef::Y | AxisDef::NegY) {
                d.warn(name, format!("fill along {} on a flat {}: it has no height; use U, V, X or Z", fill.axis.name(), layer.shape.kind_name()));
            }
        }
        let encloses = matches!(
            layer.shape,
            ShapeDef::Sphere { .. } | ShapeDef::Capsule { .. } | ShapeDef::Cylinder { open_ends: false, .. }
        ) && layer.transform.offset.length() < layer.shape.size() * 0.5;
        if m.blend == EffectBlendDef::Blend && encloses {
            let v = m.evaluate(&lookup(&param_values(project, &[])));
            let cover = shape_opacity(&v, 1.0);
            if cover >= OPAQUE_OPACITY {
                d.warn(
                    name,
                    format!(
                        "an opaque Blend {} (opacity {}) around the target hides the character inside it; lower the colour's opacity or use Add",
                        layer.shape.kind_name().to_lowercase(),
                        fmt_num(cover)
                    ),
                );
            }
        }
    }
}

/// The two triggers of a `play: Between`: both declared, not empty.
pub(crate) fn check_between(d: &mut Diagnostics, project: &ProjectV2, name: &str, play: &PlayDef) {
    let PlayDef::Between { start, stop } = play else { return };
    if start.is_empty() || stop.is_empty() {
        d.error(name, "play Between needs a start and a stop trigger");
    }
    for trigger in play.triggers().into_iter().filter(|t| !t.is_empty()) {
        if project.trigger(trigger).is_none() {
            d.error(name, format!("plays on trigger \"{}\", which is not a trigger of this package", trigger));
        }
    }
}

/// Keys of one of a layer's own curves: present, sorted, and the
/// parameter it reads.
pub(crate) fn check_keys(d: &mut Diagnostics, name: &str, what: &str, keys: Option<Vec<f32>>, param: Option<&str>, project: &ProjectV2) {
    let Some(xs) = keys else { return };
    if let Some(param) = param {
        match project.param(param).map(|p| p.kind()) {
            None => d.error(name, format!("{} reads \"{}\", which is not a parameter of this package", what, param)),
            Some(ParamKind::Color) => d.error(name, format!("{} reads \"{}\", a Color parameter; it needs a Float one", what, param)),
            Some(ParamKind::Float) => {}
        }
    }
    let over = if param.is_some() { "parameter value" } else { "age" };
    if xs.is_empty() {
        d.error(name, format!("{} curve has no keys", what));
    }
    if xs.windows(2).any(|w| w[1] < w[0]) {
        d.error(name, format!("{} curve keys must be sorted by {}", what, over));
    }
    if param.is_none() && xs.iter().any(|x| *x < 0.0) {
        d.warn(name, format!("{} curve over age has keys below 0 s; the age starts at 0", what));
    }
}

/// The sockets of a shape's or light's `attach` (`what` = "the shape"):
/// package sockets, else node names checked against the preview model.
pub(crate) fn check_attach_sockets(
    d: &mut Diagnostics,
    project: &ProjectV2,
    name: &str,
    attach: &AttachDef,
    model: Option<&PreviewModelNodes>,
    what: &str,
) {
    for socket in attach.socket_names() {
        if socket.is_empty() {
            d.error(name, "attach names an empty socket");
        } else if project.socket(socket).is_some() {
        } else if let Some(model) = model {
            if !model.names.iter().any(|n| n == socket) {
                d.error(
                    name,
                    format!("\"{}\" is neither a package socket nor a node {}; {} would sit at the target root", socket, model.place, what),
                );
            }
        } else {
            let has_model = project.preview.as_ref().is_some_and(|p| p.model.is_some());
            d.note(
                name,
                format!(
                    "\"{}\" is not a package socket, so it must be a node name in the target's model{}; until it is found {} sits at the target root",
                    socket,
                    if has_model { "" } else { " (no preview model to check against)" },
                    what
                ),
            );
        }
    }
    if let AttachDef::Segment { from, to } = attach {
        if from == to {
            d.warn(name, format!("segment from \"{}\" to itself has no length (its middle is that socket)", from));
        }
    }
}

fn follow_text(follow: FollowDef) -> &'static str {
    match follow {
        FollowDef::Full => "Full — moves, turns and tilts with its attach point",
        FollowDef::Position => "Position — moves with its attach point, keeps its own orientation (world axes)",
        FollowDef::PositionYaw => "PositionYaw — moves and turns to face where the target faces, never tilts (stays level)",
    }
}

pub(crate) fn attach_words(project: &ProjectV2, attach: &AttachDef) -> String {
    let socket = |name: &str| match project.socket(name) {
        Some(s) => match &s.node {
            Some(node) => format!("socket \"{}\" (node \"{}\" + offset {:?})", name, node, s.offset.to_array()),
            None => format!("socket \"{}\" (target root + offset {:?})", name, s.offset.to_array()),
        },
        None => format!("node \"{}\" of the target's model (target root until found)", name),
    };
    match attach {
        AttachDef::Target => "the target root".into(),
        AttachDef::Socket(name) => socket(name),
        AttachDef::Segment { from, to } => format!(
            "the middle of the segment from {} to {}, its Y axis along the segment",
            socket(from),
            socket(to)
        ),
    }
}

/// How the shape plays, in words.
fn play_text(project: &ProjectV2, layer: &ShapeLayerDef) -> String {
    let fades = format!("fades in {} s, out {} s", fmt_num(layer.fade.fade_in), fmt_num(layer.fade.fade_out));
    match (&layer.play, layer.one_shot_duration()) {
        (PlayDef::OnStart, Some(d)) => format!(
            "OnStart one-shot: plays {} s when the effect starts ({} inside that time), then is gone; the effect finishes by itself if every layer is one-shot",
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
        (PlayDef::OnTrigger(t), None) => format!(
            "OnTrigger(\"{}\") without a duration: appears at the first trigger and stays until the effect stops ({}; each trigger starts the age over)",
            t, fades
        ),
        (PlayDef::While(p, threshold), _) => format!(
            "While(\"{}\", {}): shows while {} ≥ {} ({}; the age starts over each time it switches on{})",
            p,
            fmt_num(*threshold),
            p,
            fmt_num(*threshold),
            fades,
            layer.duration.map(|d| format!(", and every {} s", fmt_num(d))).unwrap_or_default()
        ),
        (PlayDef::Between { .. }, _) => format!(
            "{}: shows {} ({}; the age starts over each time it switches on{})",
            between_words(&layer.play),
            between_when(project, &layer.play),
            fades,
            layer.duration.map(|d| format!(", and every {} s", fmt_num(d))).unwrap_or_default()
        ),
        (PlayDef::Loop | PlayDef::OnStart, _) => format!(
            "{}: shows while the effect runs ({} with the effect{})",
            if layer.play == PlayDef::Loop { "Loop" } else { "OnStart (no duration)" },
            fades,
            layer.duration.map(|d| format!("; the age starts over every {} s, so age curves repeat", fmt_num(d))).unwrap_or_default()
        ),
    }
}

/// `Between(start: "a", stop: "b")`.
pub(crate) fn between_words(play: &PlayDef) -> String {
    match play {
        PlayDef::Between { start, stop } => format!("Between(start: \"{}\", stop: \"{}\")", start, stop),
        other => other.kind_name().to_string(),
    }
}

/// When a `Between` layer is on, in words (unknown triggers marked).
pub(crate) fn between_when(project: &ProjectV2, play: &PlayDef) -> String {
    let mark = |t: &str| if project.trigger(t).is_some() { String::new() } else { " (NOT a trigger of this package)".to_string() };
    match play {
        PlayDef::Between { start, stop } if start == stop => format!(
            "from one \"{}\"{} to the next (each one switches it on or off)",
            start,
            mark(start)
        ),
        PlayDef::Between { start, stop } => format!(
            "from the moment the game fires \"{}\"{} until it fires \"{}\"{}",
            start,
            mark(start),
            stop,
            mark(stop)
        ),
        _ => String::new(),
    }
}

pub(crate) fn binding_words<T: Copy>(b: &Binding<T>, value: impl Fn(T) -> String, unit: &str) -> String {
    match b {
        Binding::Const(v) => format!("{}{}", value(*v), unit),
        Binding::Param(p, keys) => {
            let keys: Vec<String> = keys.iter().map(|(x, v)| format!("{}→{}{}", fmt_num(*x), value(*v), unit)).collect();
            format!("curve over \"{}\": {}", p, keys.join(", "))
        }
        Binding::Age(keys) => {
            let keys: Vec<String> = keys.iter().map(|(x, v)| format!("{} s→{}{}", fmt_num(*x), value(*v), unit)).collect();
            format!("over its age: {}", keys.join(", "))
        }
    }
}

/// The description of a shape layer for `analyze_project`.
pub fn describe_shape_layer(project: &ProjectV2, index: usize, layer: &ShapeLayerDef, out: &mut String) {
    let _ = writeln!(out, "\n## [{}] \"{}\" — shape layer: {}", index, layer.name, layer.shape.describe());
    let _ = writeln!(out, "  attach: {}; follow {}", attach_words(project, &layer.attach), follow_text(layer.follow));
    let t = &layer.transform;
    if t.offset != Vec3::ZERO || t.rotation_deg != Vec3::ZERO {
        let _ = writeln!(
            out,
            "  placed: offset ({}, {}, {}) m (× the size fit), rotation ({}, {}, {})° about X, Y, Z",
            fmt_num(t.offset.x),
            fmt_num(t.offset.y),
            fmt_num(t.offset.z),
            fmt_num(t.rotation_deg.x),
            fmt_num(t.rotation_deg.y),
            fmt_num(t.rotation_deg.z)
        );
    }
    let _ = writeln!(out, "  play: {}", play_text(project, layer));
    let _ = writeln!(out, "  scale: {}", binding_words(&layer.scale, scale_text, ""));
    if !matches!(layer.rotation_speed, Binding::Const(s) if s == 0.0) {
        let axis = layer.rotation_axis.normalize_or(Vec3::Y);
        let _ = writeln!(
            out,
            "  spin: {} around ({}, {}, {}) of the shape",
            binding_words(&layer.rotation_speed, fmt_num, "°/s"),
            fmt_num(axis.x),
            fmt_num(axis.y),
            fmt_num(axis.z)
        );
    }

    let defaults = param_values(project, &[]);
    let v = layer.material.evaluate(&lookup(&defaults));
    let m = &layer.material;
    let blend = match m.blend {
        EffectBlendDef::Blend => "Blend (covers what is behind by its opacity)",
        EffectBlendDef::Add => "Add (adds light, never darkens)",
    };
    let _ = writeln!(out, "  look: {}; colour {} × emissive {}; opacity {}", blend, color_text(v.color), fmt_num(v.emissive), fmt_num(v.opacity));
    describe_material(project, m, &v, MaterialOn::Shape, out);
    if layer.sort_bias != 0.0 {
        let _ = writeln!(
            out,
            "  draw order: sort bias {} m ({} see-through things at a similar distance)",
            fmt_num(layer.sort_bias),
            if layer.sort_bias > 0.0 { "drawn after, over" } else { "drawn before, under" }
        );
    }
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

    // Over its age.
    let ages = sample_ages(layer);
    if !ages.is_empty() {
        let label = if layer.is_one_shot() { "over one play" } else { "over its age" };
        let _ = writeln!(out, "  {} (default parameters):", label);
        for age in ages {
            let a = layer.binding_age(age);
            let values = m.evaluate_at(&lookup(&defaults), a);
            let scale = vector_value(&layer.scale, &lookup(&defaults), a, Vec3::ONE);
            let _ = writeln!(out, "    at {} s: {}", fmt_num(age), shape_summary(layer, &values, scale, layer.envelope(age)));
        }
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
                    let scale = vector_value(&layer.scale, &lookup(&values), 0.0, Vec3::ONE);
                    shape_summary(layer, &m.evaluate(&lookup(&values)), scale, 1.0)
                };
                let _ = writeln!(out, "  at {}={} (min): {}", name, fmt_num(p.min), at(p.min));
                let _ = writeln!(out, "  at {}={} (max): {}", name, fmt_num(p.max), at(p.max));
            }
        }
    }
}

/// The shape layers' look at each parameter set (the "Parameter sets"
/// section), at age 0.
pub fn shape_sets_report(project: &ProjectV2, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    for (_, layer) in project.shape_layers() {
        if only_effect.is_some_and(|o| o != layer.name) {
            continue;
        }
        let rows: Vec<String> = sets
            .iter()
            .map(|(_, set)| {
                let values = param_values(project, set);
                let scale = vector_value(&layer.scale, &lookup(&values), 0.0, Vec3::ONE);
                shape_summary(layer, &layer.material.evaluate(&lookup(&values)), scale, 1.0)
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

/// How a project without particle layers runs, in words, when it has shape
/// layers.
pub fn shapes_lifecycle_text(project: &ProjectV2) -> Option<String> {
    let shapes: Vec<&ShapeLayerDef> = project.shape_layers().map(|(_, l)| l).collect();
    if shapes.is_empty() {
        return None;
    }
    if project.surface_layers().next().is_some() {
        return Some("surface layers play until the game stops the effect".into());
    }
    let one_shots: Option<Vec<f32>> = shapes
        .iter()
        .map(|s| match (&s.play, s.one_shot_duration()) {
            (PlayDef::OnStart, Some(d)) => Some(d),
            _ => None,
        })
        .collect();
    Some(match one_shots {
        Some(ends) => format!(
            "one-shot: the last shape ends {} s after the start; the effect then finishes by itself",
            fmt_num(ends.into_iter().fold(0.0, f32::max))
        ),
        None => {
            let triggered: Vec<String> = shapes
                .iter()
                .filter_map(|s| match (&s.play, s.one_shot_duration()) {
                    (PlayDef::OnTrigger(t), Some(d)) => Some(format!("\"{}\" plays {} s on \"{}\"", s.name, fmt_num(d), t)),
                    _ => None,
                })
                .collect();
            format!(
                "runs until the game stops it{}",
                if triggered.is_empty() { String::new() } else { format!("; {}", triggered.join(", ")) }
            )
        }
    })
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
                parameters: [(name: "power", default: Float(0.5))],
                triggers: [(name: "slam")],
                assets: [(id: "noise", kind: Texture((path: "omagari://textures/noise.png", purpose: Noise)))],
                layers: [Shape(({}))],
            )"#,
            layer
        );
        ProjectV2::from_ron_str(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    const WAVE: &str = r#"name: "wave",
        shape: Ring(inner: 0.8, outer: 1.0),
        follow: PositionYaw,
        transform: (offset: (0.0, -0.86, 0.0)),
        scale: Age([(0.0, (0.2, 1.0, 0.2)), (0.6, (3.0, 1.0, 3.0))]),
        play: OnTrigger("slam"),
        duration: Some(0.6),
        fade: (in: 0.0, out: 0.3),
        material: (blend: Add, color: Const((0.6, 1.2, 2.5, 1.0)), opacity: Param("power", [(0.0, 0.0), (1.0, 1.0)]),
            noise_a: Some((texture: (asset: "noise"), mapping: Uv, scale: Const(3.0), strength: Const(0.4))),
            fill: Some((axis: V, from: Const(0.0), to: Const(1.0), softness: Const(0.5)))),"#;

    #[test]
    fn a_good_shape_layer_passes_and_is_described() {
        let p = project(WAVE);
        let d = check_project(&p, &root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root(), 50, None);
        for needle in [
            "## [0] \"wave\" — shape layer: flat ring in XZ (facing up), from 0.8 m to 1 m from the centre",
            "follow PositionYaw — moves and turns to face where the target faces, never tilts",
            "OnTrigger(\"slam\") one-shot: each time the game fires \"slam\" it plays 0.60 s from age 0",
            "scale: over its age: 0 s→(0.20, 1, 0.20), 0.60 s→(3, 1, 3)",
            "over one play (default parameters):",
            "at 0 s: ~0.40 m across (scale (0.20, 1, 0.20))",
            "at 0.30 s: ~3.20 m across",
            "at 0.59 s: ~5.89 m across (scale (2.94, 1, 2.94)), colour blue×2.50 a1, opacity ~0.016",
            "by the shape's UVs",
            "fill: shows the band 0..1 of the shape along V (across the mesh)",
            "at power=1 (max):",
            "runs until the game stops it; \"wave\" plays 0.60 s on \"slam\"",
            "trigger \"slam\": bursts wave",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
    }

    #[test]
    fn shape_problems_are_reported() {
        let bad = r#"name: "bad",
            shape: Ring(inner: 1.0, outer: 0.5),
            scale: Age([(0.0, (1.0, 1.0, 1.0)), (2.0, (40.0, 40.0, 40.0))]),
            play: OnTrigger("boom"),
            material: (blend: Add, fill: Some((axis: Y))),"#;
        let d = check_project(&project(bad), &root());
        for e in [
            "[bad] Ring outer - inner -0.50 must be above 0",
            "[bad] ring inner radius 1 must be below outer 0.50",
            "[bad] plays on trigger \"boom\", which is not a trigger of this package",
        ] {
            assert!(has(&d.errors, e), "missing error {:?}\n{}", e, d.render());
        }
        for w in [
            "[bad] the shape gets ~40 m across",
            "[bad] plays on a trigger but has no duration",
            "[bad] fill along Y (up) on a flat Ring",
        ] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }

        let late = r#"name: "late", play: OnStart, duration: Some(0.5), fade: (in: 0.4, out: 0.4),
            material: (blend: Add, opacity: Age([(0.0, 1.0), (1.0, 0.0)])),"#;
        let d = check_project(&project(late), &root());
        assert!(has(&d.warnings, "[late] opacity curve over age has keys up to 1 s, past the duration 0.50 s: the shape is gone"), "{}", d.render());
        assert!(has(&d.warnings, "[late] fade in 0.40 + out 0.40 s is longer than the duration 0.50 s"), "{}", d.render());
        let report = analyze_project(&project(late), &root(), 50, None);
        assert!(report.contains("one-shot: the last shape ends 0.50 s after the start"), "{}", report);

        let opaque = r#"name: "ball", shape: Sphere(radius: 1.0), material: (blend: Blend, color: Const((1.0, 0.0, 0.0, 1.0)))"#;
        let d = check_project(&project(opaque), &root());
        assert!(has(&d.warnings, "[ball] an opaque Blend sphere (opacity 1) around the target hides the character"), "{}", d.render());

        let socket = r#"name: "s", attach: Socket("mixamorig:Hips"), play: Loop, duration: None, scale: Age([(0.0, (1.0, 1.0, 1.0)), (1.0, (2.0, 2.0, 2.0))])"#;
        let d = check_project(&project(socket), &root());
        assert!(has(&d.notes, "[s] \"mixamorig:Hips\" is not a package socket"), "{}", d.render());
        assert!(has(&d.notes, "[s] curves over age run once"), "{}", d.render());
    }
}
