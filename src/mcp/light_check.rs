//! Checks and descriptions of light layers (P6, F7) for `check_project` and
//! `analyze_project`. Lights are not simulated: their brightness is worked
//! out from the bindings at parameter values and over their age, and
//! described in lumens and in lux at 1 m.

use std::fmt::Write;

use bevy::prelude::*;
use omagari::format::*;

use super::analysis::{color_name, fmt_num, param_values, Diagnostics, ParamSet, PreviewModelNodes};
use super::shape_check::{attach_words, between_when, between_words, binding_words, check_attach_sockets, check_between, check_keys};
use super::surface_check::lookup;

/// Above this many lumens a light is reported as very bright (a hit flash
/// seen from the game camera needs ~50 000–200 000).
pub const HUGE_LIGHT_INTENSITY: f32 = 1_000_000.0;
/// Below this many lumens (at its brightest) a light hardly shows.
pub const DIM_LIGHT_INTENSITY: f32 = 200.0;
/// Ranges above this (metres) are reported as costly.
pub const LONG_LIGHT_RANGE: f32 = 25.0;
/// More light layers than this in one package get a warning (the game's
/// global cap is 8 by default, shared by every effect).
pub const MAX_LIGHTS_PER_PACKAGE: usize = 3;
/// The editor's sun on the floor (lux), for comparisons in the descriptions.
const EDITOR_SUN_ON_FLOOR: f32 = 4600.0;

/// Every value a number binding can take (the constant, or every key).
fn values_of(b: &Binding<f32>) -> Vec<f32> {
    match b {
        Binding::Const(v) => vec![*v],
        Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
    }
}

/// Illuminance (lux) `distance` metres from a point light of `lumens`
/// (ignoring the smooth cut-off near its range).
pub fn lux_at(lumens: f32, distance: f32) -> f32 {
    lumens / (4.0 * std::f32::consts::PI * distance.max(0.01).powi(2))
}

/// The light's look at one moment, in words.
fn light_summary(values: &LightValues, level: f32) -> String {
    let lm = values.intensity * level;
    if lm <= 0.0 || values.range <= 0.0 {
        return "dark".into();
    }
    format!(
        "{} lm ({} lux at 1 m), {}, range {} m",
        fmt_num(lm.round()),
        fmt_num(lux_at(lm, 1.0).round()),
        color_name(values.color),
        fmt_num(values.range)
    )
}

/// The ages a description samples: over the one-shot / repeat duration, or
/// up to the last `Age` key.
fn sample_ages(layer: &LightLayerDef) -> Vec<f32> {
    let span = match layer.duration {
        Some(d) if d > 0.0 => Some(d),
        _ => layer.age_bindings().iter().map(|(_, last)| *last).fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b)))),
    };
    let Some(span) = span.filter(|s| *s > 0.0) else { return Vec::new() };
    let mut ages: Vec<f32> = [0.0, 0.1, 0.25, 0.5, 0.75].iter().map(|f| f * span).collect();
    ages.push(if layer.is_one_shot() { span * 0.98 } else { span });
    ages.dedup();
    ages
}

/// Every check of the light layers.
pub fn check_light_layers(d: &mut Diagnostics, project: &ProjectV2, model: Option<&PreviewModelNodes>) {
    let count = project.light_layers().count();
    if count > MAX_LIGHTS_PER_PACKAGE {
        d.warnings.push(format!(
            "{} light layers in one package: the game shows at most 8 Omagari lights at once by default (VfxSettings::max_lights, shared by every effect), so with a few copies of this effect the weakest go dark; keep one to three lights per effect",
            count
        ));
    }
    for (index, layer) in project.light_layers() {
        let name = layer.name.as_str();
        if name.trim().is_empty() {
            d.errors.push("a light layer has an empty name".into());
        }
        if project.layers.iter().enumerate().any(|(i, l)| i != index && l.name() == name) {
            d.error(name, "duplicate layer name; names must be unique");
        }
        check_attach_sockets(d, project, name, &layer.attach, model, "the light");

        // Colour.
        match &layer.color {
            Binding::Param(param, keys) => match project.param(param).map(|p| p.kind()) {
                None => d.error(name, format!("color reads \"{}\", which is not a parameter of this package", param)),
                Some(ParamKind::Color) => {
                    if !keys.is_empty() {
                        d.warn(name, format!("color reads the Color parameter \"{}\" directly; its keys are not used", param));
                    }
                }
                Some(ParamKind::Float) => check_keys(d, name, "color", Some(keys.iter().map(|k| k.0).collect()), Some(param), project),
            },
            Binding::Age(keys) => check_keys(d, name, "color", Some(keys.iter().map(|k| k.0).collect()), None, project),
            Binding::Const(_) => {}
        }
        let colors: Vec<Vec4> = match &layer.color {
            Binding::Const(c) => vec![*c],
            Binding::Param(_, keys) | Binding::Age(keys) => keys.iter().map(|k| k.1).collect(),
        };
        if !colors.is_empty() && colors.iter().all(|c| c.truncate().max_element() <= 0.0) {
            d.warn(name, "the colour is black: the light lights nothing");
        }

        // Intensity and range.
        for (what, b) in [("intensity", &layer.intensity), ("range", &layer.range)] {
            let keys = b.keys().map(|k| k.iter().map(|k| k.0).collect::<Vec<_>>());
            check_keys(d, name, what, keys, b.param(), project);
        }
        let intensities = values_of(&layer.intensity);
        if intensities.iter().any(|v| *v < 0.0) {
            d.error(name, "intensity has a value below 0 lumens");
        }
        let peak = intensities.iter().cloned().fold(0.0f32, f32::max);
        if !intensities.is_empty() && peak <= 0.0 {
            d.warn(name, "intensity is 0 everywhere: the light never shines");
        } else if peak > 0.0 && peak < DIM_LIGHT_INTENSITY {
            d.warn(
                name,
                format!(
                    "at most {} lm: too dim to see from a game camera (Bevy's unit is lumens; a 60 W bulb ≈ 800 lm, an effect glow 3 000–60 000)",
                    fmt_num(peak)
                ),
            );
        }
        if peak > HUGE_LIGHT_INTENSITY {
            d.warn(
                name,
                format!("up to {} lm: blinding (a hit flash needs ~50 000–200 000); check the units (lumens)", fmt_num(peak)),
            );
        }
        let ranges = values_of(&layer.range);
        if matches!(layer.range, Binding::Const(r) if r <= 0.0) {
            d.error(name, "range must be above 0 metres: nothing is lit");
        } else if ranges.iter().any(|r| *r < 0.0) {
            d.error(name, "range has a value below 0 metres");
        }
        let longest = ranges.iter().cloned().fold(0.0f32, f32::max);
        if longest > LONG_LIGHT_RANGE {
            d.warn(
                name,
                format!(
                    "range up to {} m: every pixel within it is shaded by this light (costly); a character effect needs 2–6 m",
                    fmt_num(longest)
                ),
            );
        }
        if layer.radius < 0.0 {
            d.error(name, format!("radius {} must be 0 or more metres", fmt_num(layer.radius)));
        } else if longest > 0.0 && layer.radius > longest {
            d.warn(name, format!("radius {} m is larger than the range {} m", fmt_num(layer.radius), fmt_num(longest)));
        }

        // Flicker.
        let f = layer.flicker;
        if !(0.0..=1.0).contains(&f.amount) {
            d.warn(name, format!("flicker amount {} is outside 0..1 (a fraction of the intensity; 1 = dark to twice as bright)", fmt_num(f.amount)));
        }
        if f.amount > 0.0 && f.speed <= 0.0 {
            d.warn(name, "flicker amount is set but its speed is 0: it does not flicker");
        } else if f.amount > 0.0 && f.speed > 40.0 {
            d.warn(name, format!("flicker speed {} changes per second strobes; a flame is 4–8, a crackle 15–25", fmt_num(f.speed)));
        }

        if layer.shadows {
            d.warn(
                name,
                "shadows: true renders six shadow maps for this light every frame (costly, per copy of the effect); effect lights rarely need them",
            );
        }

        // Play, duration and age.
        match &layer.play {
            PlayDef::OnTrigger(trigger) => {
                if project.trigger(trigger).is_none() {
                    d.error(name, format!("plays on trigger \"{}\", which is not a trigger of this package", trigger));
                }
                if layer.duration.is_none() {
                    d.warn(
                        name,
                        "plays on a trigger but has no duration: after the first trigger it stays on until the effect stops; set duration (seconds) for a flash",
                    );
                }
            }
            PlayDef::Between { .. } => check_between(d, project, name, &layer.play),
            PlayDef::While(param, _) => match project.param(param).map(|p| p.kind()) {
                None => d.error(name, format!("play While reads \"{}\", which is not a parameter of this package", param)),
                Some(ParamKind::Color) => d.error(name, format!("play While reads \"{}\", a Color parameter; it needs a Float one", param)),
                Some(ParamKind::Float) => {}
            },
            PlayDef::OnStart if layer.duration.is_none() => d.note(
                name,
                "OnStart without a duration shines like Loop (until the effect stops); set duration for a one-shot",
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
                            "fade in {} + out {} s is longer than the duration {} s: the light never reaches full brightness",
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
                                if layer.is_one_shot() { "the light is off before they are reached" } else { "the age starts over before they are reached" }
                            ),
                        );
                    }
                }
            }
        } else if !layer.is_one_shot() && !layer.age_bindings().is_empty() && !matches!(layer.play, PlayDef::OnTrigger(_)) {
            d.note(
                name,
                "curves over age run once from the start and then hold their last key; give the light a duration to repeat them",
            );
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

fn play_text(project: &ProjectV2, layer: &LightLayerDef) -> String {
    let fades = format!("fades in {} s, out {} s", fmt_num(layer.fade.fade_in), fmt_num(layer.fade.fade_out));
    match (&layer.play, layer.one_shot_duration()) {
        (PlayDef::OnStart, Some(d)) => format!(
            "OnStart one-shot: shines {} s when the effect starts ({} inside that time), then is off; the effect finishes by itself if every layer is a one-shot",
            fmt_num(d),
            fades
        ),
        (PlayDef::OnTrigger(t), Some(d)) => {
            let known = if project.trigger(t).is_some() { "" } else { " (NOT a trigger of this package)" };
            format!(
                "OnTrigger(\"{}\"){} flash: each time the game fires \"{}\" it shines {} s from age 0 ({} inside that time; a trigger during the flash starts it over; with trigger_at it shines at the payload position)",
                t, known, t, fmt_num(d), fades
            )
        }
        (PlayDef::OnTrigger(t), None) => format!(
            "OnTrigger(\"{}\") without a duration: turns on at the first trigger and stays on until the effect stops ({})",
            t, fades
        ),
        (PlayDef::While(p, threshold), _) => format!(
            "While(\"{}\", {}): shines while {} ≥ {} ({}; the age starts over each time it switches on)",
            p,
            fmt_num(*threshold),
            p,
            fmt_num(*threshold),
            fades
        ),
        (PlayDef::Between { .. }, _) => format!(
            "{}: shines {} ({}; the age starts over each time it switches on)",
            between_words(&layer.play),
            between_when(project, &layer.play),
            fades
        ),
        (PlayDef::Loop | PlayDef::OnStart, _) => format!(
            "{}: shines while the effect runs ({} with the effect)",
            if layer.play == PlayDef::Loop { "Loop" } else { "OnStart (no duration)" },
            fades
        ),
    }
}

fn color_words(b: &Binding<Vec4>, project: &ProjectV2) -> String {
    match b {
        Binding::Param(p, keys) if keys.is_empty() || project.param(p).is_some_and(|p| p.kind() == ParamKind::Color) => {
            format!("the Color parameter \"{}\"", p)
        }
        _ => binding_words(b, |c: Vec4| color_name(c.truncate()), ""),
    }
}

/// The description of a light layer for `analyze_project`.
pub fn describe_light_layer(project: &ProjectV2, index: usize, layer: &LightLayerDef, out: &mut String) {
    let _ = writeln!(out, "\n## [{}] \"{}\" — light layer: a point light at {}", index, layer.name, attach_words(project, &layer.attach));
    if layer.offset != Vec3::ZERO {
        let o = layer.offset;
        let _ = writeln!(
            out,
            "  placed: offset ({}, {}, {}) m in the attach point's space (× the size fit)",
            fmt_num(o.x),
            fmt_num(o.y),
            fmt_num(o.z)
        );
    }
    let _ = writeln!(out, "  play: {}", play_text(project, layer));
    let _ = writeln!(out, "  colour: {}", color_words(&layer.color, project));
    let _ = writeln!(out, "  intensity: {} (lumens)", binding_words(&layer.intensity, |v: f32| fmt_num(v), " lm"));
    let _ = writeln!(out, "  range: {}; source radius {} m", binding_words(&layer.range, |v: f32| fmt_num(v), " m"), fmt_num(layer.radius));
    if layer.flicker.is_on() {
        let (lo, hi) = layer.flicker.bounds();
        let _ = writeln!(
            out,
            "  flicker: smooth random changes between {}% and {}% of the intensity, about {} per second (different for each copy of the effect)",
            fmt_num(lo * 100.0),
            fmt_num(hi * 100.0),
            fmt_num(layer.flicker.speed)
        );
    }
    let _ = writeln!(
        out,
        "  shadows: {}",
        if layer.shadows { "on (six shadow maps per frame: costly)" } else { "off" }
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

    let defaults = param_values(project, &[]);
    let v = layer.values_at(&lookup(&defaults), 0.0);
    let _ = writeln!(
        out,
        "  brightness (default parameters, age 0): {}{}",
        light_summary(&v, 1.0),
        if v.intensity > 0.0 && v.range > 0.0 {
            format!(
                "; the floor 1 m below gets ~{}% of the editor sun's light (it shows from ~10%)",
                fmt_num((lux_at(v.intensity, 1.0) / EDITOR_SUN_ON_FLOOR * 100.0).round())
            )
        } else {
            String::new()
        }
    );
    let ages = sample_ages(layer);
    if !ages.is_empty() {
        let label = if layer.is_one_shot() { "over one flash" } else { "over its age" };
        let _ = writeln!(out, "  {} (default parameters):", label);
        for age in ages {
            let values = layer.values_at(&lookup(&defaults), layer.binding_age(age));
            let _ = writeln!(out, "    at {} s: {}", fmt_num(age), light_summary(&values, layer.envelope(age)));
        }
    }
    for name in layer.params_used() {
        let Some(p) = project.param(&name) else {
            let _ = writeln!(out, "  reads \"{}\", which is not a parameter", name);
            continue;
        };
        match p.kind() {
            ParamKind::Color => {
                let _ = writeln!(out, "  colour follows the Color parameter \"{}\"", name);
            }
            ParamKind::Float => {
                let at = |x: f32| {
                    let values = param_values(project, &[(name.clone(), ParamValue::Float(x))]);
                    let level = layer.visible.as_ref().filter(|v| v.param == name).map(|v| v.factor(x)).unwrap_or(1.0);
                    let on = match &layer.play {
                        PlayDef::While(param, t) if *param == name => x >= *t,
                        _ => true,
                    };
                    light_summary(&layer.values_at(&lookup(&values), 0.0), if on { level } else { 0.0 })
                };
                let _ = writeln!(out, "  at {}={} (min): {}", name, fmt_num(p.min), at(p.min));
                let _ = writeln!(out, "  at {}={} (middle): {}", name, fmt_num(p.middle()), at(p.middle()));
                let _ = writeln!(out, "  at {}={} (max): {}", name, fmt_num(p.max), at(p.max));
            }
        }
    }
    let _ = writeln!(
        out,
        "  size fit: range and radius × the fit, intensity × fit² (a scaled-up copy lights its surroundings the same way); the game's global cap (VfxSettings::max_lights, default 8) switches the weakest Omagari lights off"
    );
}

/// The light layers' brightness at each parameter set (the "Parameter sets"
/// section), at age 0.
pub fn light_sets_report(project: &ProjectV2, sets: &[(String, ParamSet)], only_effect: Option<&str>, out: &mut String) {
    for (_, layer) in project.light_layers() {
        if only_effect.is_some_and(|o| o != layer.name) {
            continue;
        }
        let rows: Vec<String> = sets
            .iter()
            .map(|(_, set)| {
                let values = param_values(project, set);
                let x = |name: &str| values.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_float());
                let mut level = layer.visible.as_ref().and_then(|v| x(&v.param).map(|f| v.factor(f))).unwrap_or(1.0);
                if let PlayDef::While(param, t) = &layer.play {
                    if !x(param).is_some_and(|v| v >= *t) {
                        level = 0.0;
                    }
                }
                light_summary(&layer.values_at(&lookup(&values), 0.0), level)
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

/// When the lights shine, in words ("Whole project").
pub fn lights_lifecycle_text(project: &ProjectV2) -> Option<String> {
    let parts: Vec<String> = project
        .light_layers()
        .map(|(_, l)| match (&l.play, l.one_shot_duration()) {
            (PlayDef::OnStart, Some(d)) => format!("\"{}\" shines {} s after the start", l.name, fmt_num(d)),
            (PlayDef::OnTrigger(t), Some(d)) => format!("\"{}\" flashes {} s on \"{}\"", l.name, fmt_num(d), t),
            (PlayDef::Between { .. }, _) => format!("\"{}\" shines {}", l.name, between_when(project, &l.play)),
            (PlayDef::While(p, t), _) => format!("\"{}\" shines while {} ≥ {}", l.name, p, fmt_num(*t)),
            _ => format!("\"{}\" shines while the effect runs", l.name),
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!("lights: {}", parts.join(", ")))
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
                parameters: [(name: "charge", default: Float(0.0))],
                triggers: [(name: "hit"), (name: "swing_start"), (name: "swing_end")],
                sockets: [(name: "hand", node: None, offset: (0.3, 1.0, 0.0))],
                layers: [{}],
            )"#,
            layers
        );
        ProjectV2::from_ron_str(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text))
    }

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    const GLOW: &str = r#"Light((name: "glow", attach: Socket("hand"),
        color: Const((0.45, 0.7, 1.0, 1.0)),
        intensity: Param("charge", [(0.0, 0.0), (1.0, 40000.0)]),
        range: Const(4.0),
        flicker: (amount: 0.1, speed: 6.0))),
        Light((name: "flash", play: OnTrigger("hit"), duration: Some(0.25), fade: (in: 0.0, out: 0.05),
        color: Const((1.0, 0.85, 0.6, 1.0)),
        intensity: Age([(0.0, 150000.0), (0.25, 0.0)]),
        range: Const(5.0))),
        Light((name: "swing", play: Between(start: "swing_start", stop: "swing_end"), intensity: Const(10000.0), range: Const(3.0)))"#;

    #[test]
    fn good_lights_pass_and_are_described() {
        let p = project(GLOW);
        let d = check_project(&p, &root());
        assert!(d.errors.is_empty() && d.warnings.is_empty() && d.notes.is_empty(), "{}", d.render());
        let report = analyze_project(&p, &root(), 50, None);
        for needle in [
            "## [0] \"glow\" — light layer: a point light at socket \"hand\" (target root + offset",
            "intensity: curve over \"charge\": 0→0 lm, 1→40000 lm (lumens)",
            "flicker: smooth random changes between 90% and 110% of the intensity, about 6 per second",
            "at charge=0 (min): dark",
            "at charge=1 (max): 40000 lm (3183 lux at 1 m), blue, range 4 m",
            "OnTrigger(\"hit\") flash: each time the game fires \"hit\" it shines 0.25 s from age 0",
            "over one flash (default parameters):",
            "at 0 s: 150000 lm (11937 lux at 1 m)",
            "Between(start: \"swing_start\", stop: \"swing_end\"): shines from the moment the game fires \"swing_start\" until it fires \"swing_end\"",
            "lights: \"glow\" shines while the effect runs, \"flash\" flashes 0.25 s on \"hit\"",
            "trigger \"hit\": bursts flash",
            "trigger \"swing_start\": starts swing",
            "trigger \"swing_end\": stops swing",
            "[glow] changes with the parameters:",
        ] {
            assert!(report.contains(needle), "missing {:?} in\n{}", needle, report);
        }
    }

    #[test]
    fn light_problems_are_reported() {
        let bad = r#"Light((name: "bad", attach: Socket("nowhere"), play: OnTrigger("boom"), intensity: Const(50.0), range: Const(0.0),
                radius: -1.0, flicker: (amount: 1.5, speed: 0.0), shadows: true,
                color: Param("nope", [(0.0, (1.0, 1.0, 1.0, 1.0))]))),
            Light((name: "b", play: Between(start: "swing_start", stop: "gone"), intensity: Const(3000000.0), range: Const(60.0))),
            Light((name: "c", intensity: Param("missing", [(0.0, 1.0), (1.0, 2000.0)]))),
            Light((name: "d"))"#;
        let d = check_project(&project(bad), &root());
        for e in [
            "[bad] plays on trigger \"boom\", which is not a trigger of this package",
            "[bad] range must be above 0 metres",
            "[bad] radius -1 must be 0 or more metres",
            "[bad] color reads \"nope\", which is not a parameter of this package",
            "[b] plays on trigger \"gone\", which is not a trigger of this package",
            "[c] intensity reads \"missing\", which is not a parameter of this package",
        ] {
            assert!(has(&d.errors, e), "missing error {:?}\n{}", e, d.render());
        }
        for w in [
            "4 light layers in one package",
            "[bad] at most 50 lm: too dim to see",
            "[bad] flicker amount 1.50 is outside 0..1",
            "[bad] flicker amount is set but its speed is 0",
            "[bad] shadows: true renders six shadow maps",
            "[bad] plays on a trigger but has no duration",
            "[b] up to 3000000 lm: blinding",
            "[b] range up to 60 m",
        ] {
            assert!(has(&d.warnings, w), "missing warning {:?}\n{}", w, d.render());
        }
        assert!(has(&d.notes, "[bad] \"nowhere\" is not a package socket"), "{}", d.render());
    }

    #[test]
    fn lux_follows_the_inverse_square_law() {
        assert!((lux_at(4.0 * std::f32::consts::PI, 1.0) - 1.0).abs() < 1e-5);
        assert!((lux_at(1000.0, 2.0) * 4.0 - lux_at(1000.0, 1.0)).abs() < 1e-3);
    }
}
