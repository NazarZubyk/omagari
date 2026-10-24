//! Editor widgets for a light layer (`omagari::format::LightLayerDef`):
//! presets, placement, colour / intensity / range bindings, flicker, play,
//! fades, shadows; and the viewport gizmo of the preview's lights.

use bevy::light::PointLight;
use bevy::prelude::*;
use bevy_egui::egui;
use omagari::format::*;
use omagari::runtime::{LightLayerState, VfxLightLayer};
use omagari::{VfxInstance, VfxPackage};

use super::effect::{ui_for_attach, ui_for_play, ui_for_visible};
use super::helpers::ui_for_f32_ex;
use super::hints;
use super::session::EditorSession;
use super::surface::{color_row, number_row};
use super::trail::ensure_trigger;
use super::AppContext;

const WARN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x88, 0x66);

/// The trigger the "hit flash" preset uses when the project has none.
pub const HIT: &str = "hit";

/// Ready-made lights (the inspector's preset buttons).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightPreset {
    WeaponGlow,
    HitFlash,
    TorchFlicker,
}

impl LightPreset {
    pub const ALL: [LightPreset; 3] = [LightPreset::WeaponGlow, LightPreset::HitFlash, LightPreset::TorchFlicker];

    pub fn label(self) -> &'static str {
        match self {
            LightPreset::WeaponGlow => "weapon glow",
            LightPreset::HitFlash => "hit flash",
            LightPreset::TorchFlicker => "torch flicker",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            LightPreset::WeaponGlow => "A cool blue glow that lights the floor around it, brighter as the first number parameter rises (0 at its min), with a faint shimmer",
            LightPreset::HitFlash => "A bright warm flash that fades over 0.25 s each time the first trigger fires (adds a \"hit\" trigger when there is none); at the hit point with trigger_at",
            LightPreset::TorchFlicker => "A warm orange light that flickers like a flame, always on",
        }
    }
}

/// Applies a preset to a layer: colour, intensity, range, flicker, play and
/// fades (the name, attach, offset and visible range are kept). `param` is
/// the project's first number parameter; the hit flash plays on the first
/// trigger (a `hit` trigger is added when there is none).
pub fn apply_light_preset(layer: &mut LightLayerDef, preset: LightPreset, triggers: &mut Vec<TriggerDef>, param: Option<&ParamDef>) {
    let keep = (layer.name.clone(), layer.attach.clone(), layer.offset, layer.visible.clone());
    *layer = match preset {
        LightPreset::WeaponGlow => LightLayerDef {
            color: Binding::Const(Vec4::new(0.45, 0.7, 1.0, 1.0)),
            intensity: match param {
                Some(p) => Binding::Param(p.name.clone(), vec![(p.min, 0.0), (p.max, 40000.0)]),
                None => Binding::Const(25000.0),
            },
            range: Binding::Const(4.0),
            radius: 0.05,
            flicker: LightFlickerDef { amount: 0.08, speed: 5.0 },
            fade: FadeDef { fade_in: 0.2, fade_out: 0.4 },
            ..Default::default()
        },
        LightPreset::HitFlash => {
            let trigger = match triggers.first() {
                Some(t) => t.name.clone(),
                None => {
                    ensure_trigger(triggers, HIT);
                    HIT.to_string()
                }
            };
            LightLayerDef {
                color: Binding::Const(Vec4::new(1.0, 0.85, 0.6, 1.0)),
                intensity: Binding::Age(vec![(0.0, 150000.0), (0.05, 120000.0), (0.25, 0.0)]),
                range: Binding::Const(5.0),
                radius: 0.1,
                play: PlayDef::OnTrigger(trigger),
                duration: Some(0.25),
                fade: FadeDef { fade_in: 0.0, fade_out: 0.05 },
                ..Default::default()
            }
        }
        LightPreset::TorchFlicker => LightLayerDef {
            color: Binding::Const(Vec4::new(1.0, 0.55, 0.2, 1.0)),
            intensity: Binding::Const(18000.0),
            range: Binding::Const(5.0),
            radius: 0.08,
            flicker: LightFlickerDef { amount: 0.35, speed: 7.0 },
            fade: FadeDef { fade_in: 0.3, fade_out: 0.5 },
            ..Default::default()
        },
    };
    layer.name = keep.0;
    layer.attach = keep.1;
    layer.offset = keep.2;
    layer.visible = keep.3;
}

/// A new light layer (a weapon glow on the project's first socket, or the
/// target root), with a unique name.
pub fn new_light_layer(
    triggers: &mut Vec<TriggerDef>,
    sockets: &[SocketDef],
    existing: &[String],
    param: Option<&ParamDef>,
) -> LightLayerDef {
    let mut name = "light".to_string();
    let mut n = 2;
    while existing.contains(&name) {
        name = format!("light{}", n);
        n += 1;
    }
    let attach = sockets.first().map(|s| AttachDef::Socket(s.name.clone())).unwrap_or_default();
    let mut layer = LightLayerDef { name, attach, ..Default::default() };
    apply_light_preset(&mut layer, LightPreset::WeaponGlow, triggers, param);
    layer
}

fn vec3_fields(ui: &mut egui::Ui, v: &mut Vec3, min: f32, max: f32, speed: f32, unit: &str) {
    ui.horizontal(|ui| {
        for (label, i) in [("x", 0), ("y", 1), ("z", 2)] {
            ui.label(label);
            v[i] = ui_for_f32_ex(ui, v[i], min, max, speed);
        }
        ui.label(unit);
    });
}

/// The light layer inspector.
#[allow(clippy::too_many_arguments)]
pub fn draw_light_layer(
    layer: &mut LightLayerDef,
    triggers: &mut Vec<TriggerDef>,
    parameters: &[ParamDef],
    app: &mut AppContext,
    show_lights: &mut bool,
    ui: &mut egui::Ui,
    index: u64,
) {
    let id = ui.make_persistent_id(format!("light layer {}", index));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak());
        for preset in LightPreset::ALL {
            if ui.small_button(preset.label()).on_hover_text(preset.hint()).clicked() {
                let param = parameters.iter().find(|p| p.kind() == ParamKind::Float);
                apply_light_preset(layer, preset, triggers, param);
            }
        }
    });
    ui.add_space(4.0);
    egui::Grid::new(id.with("grid")).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("Name").on_hover_text(hints::NAME);
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0)).on_hover_text(hints::NAME);
        ui.end_row();

        ui.label("Attach").on_hover_text(hints::LIGHT_ATTACH);
        ui.horizontal(|ui| ui_for_attach(ui, &mut layer.attach, &app.socket_names, id));
        ui.end_row();

        ui.label("Offset").on_hover_text(hints::LIGHT_OFFSET);
        vec3_fields(ui, &mut layer.offset, -1000.0, 1000.0, 0.01, "m");
        ui.end_row();

        color_row(ui, "Colour", hints::LIGHT_COLOR, &mut layer.color, app, parameters, id);
        number_row(ui, "Intensity", hints::LIGHT_INTENSITY, &mut layer.intensity, app, parameters, "lm", (0.0, 2.0e6), 100.0, id);
        number_row(ui, "Range", hints::LIGHT_RANGE, &mut layer.range, app, parameters, "m", (0.0, 100.0), 0.02, id);

        ui.label("Radius").on_hover_text(hints::LIGHT_RADIUS);
        ui.horizontal(|ui| {
            layer.radius = ui_for_f32_ex(ui, layer.radius, 0.0, 10.0, 0.005);
            ui.label("m");
        });
        ui.end_row();

        ui.label("Flicker").on_hover_text(hints::LIGHT_FLICKER);
        ui.horizontal(|ui| {
            ui.label("amount").on_hover_text(hints::LIGHT_FLICKER);
            layer.flicker.amount = ui_for_f32_ex(ui, layer.flicker.amount, 0.0, 1.0, 0.005);
            ui.label("speed").on_hover_text(hints::LIGHT_FLICKER);
            layer.flicker.speed = ui_for_f32_ex(ui, layer.flicker.speed, 0.0, 60.0, 0.05);
            ui.label("/s");
            if layer.flicker.is_on() {
                let (lo, hi) = layer.flicker.bounds();
                ui.label(egui::RichText::new(format!("{:.0}–{:.0}%", lo * 100.0, hi * 100.0)).weak());
            }
        });
        ui.end_row();

        ui.label("Play").on_hover_text(hints::LIGHT_PLAY);
        ui.horizontal_wrapped(|ui| ui_for_play(ui, &mut layer.play, app, parameters, id));
        ui.end_row();

        ui.label("Duration").on_hover_text(hints::LIGHT_DURATION);
        ui.horizontal(|ui| {
            let mut on = layer.duration.is_some();
            if ui.checkbox(&mut on, "").on_hover_text(hints::LIGHT_DURATION).changed() {
                layer.duration = on.then_some(0.25);
            }
            if let Some(d) = &mut layer.duration {
                *d = ui_for_f32_ex(ui, *d, 0.0, 3600.0, 0.01);
                ui.label("s");
                ui.label(egui::RichText::new(if layer.is_one_shot() { "one-shot" } else { "the age repeats" }).weak());
            } else if matches!(layer.play, PlayDef::OnTrigger(_)) {
                ui.colored_label(WARN, "⚠ off: after the first trigger it stays on until the effect stops");
            }
        });
        ui.end_row();

        ui.label("Fade").on_hover_text(hints::LIGHT_FADE);
        ui.horizontal(|ui| {
            ui.label("in");
            layer.fade.fade_in = ui_for_f32_ex(ui, layer.fade.fade_in, 0.0, 60.0, 0.01);
            ui.label("s   out");
            layer.fade.fade_out = ui_for_f32_ex(ui, layer.fade.fade_out, 0.0, 60.0, 0.01);
            ui.label("s");
        })
        .response
        .on_hover_text(hints::LIGHT_FADE);
        ui.end_row();

        ui.label("Visible").on_hover_text(hints::VISIBLE);
        ui_for_visible(ui, &mut layer.visible, app, parameters, id);
        ui.end_row();

        ui.label("Shadows").on_hover_text(hints::LIGHT_SHADOWS);
        ui.horizontal(|ui| {
            ui.checkbox(&mut layer.shadows, "cast shadows").on_hover_text(hints::LIGHT_SHADOWS);
            if layer.shadows {
                ui.colored_label(WARN, "⚠ costly");
            }
        });
        ui.end_row();

        ui.label("Viewport").on_hover_text(hints::LIGHT_GIZMO);
        ui.checkbox(show_lights, "show lights (position and range)").on_hover_text(hints::LIGHT_GIZMO);
        ui.end_row();
    });
    ui.label(
        egui::RichText::new(
            "The game shows a limited number of Omagari lights at once (default 8; the weakest are switched off). On the editor's floor the light shows on the FLOOR, CAPSULE and MODEL stages.",
        )
        .weak()
        .small(),
    );
}

/// Draws the preview's lights: a star at each light and a circle at its
/// range around it (the selected light layer brighter, with its range
/// sphere).
pub fn draw_light_gizmos(
    mut gizmos: Gizmos,
    session: Res<EditorSession>,
    packages: Res<Assets<VfxPackage>>,
    instances: Query<&VfxInstance>,
    lights: Query<(&VfxLightLayer, &LightLayerState, &GlobalTransform, &PointLight)>,
) {
    if !session.show_lights {
        return;
    }
    for (layer, state, global, light) in lights.iter() {
        let Ok(instance) = instances.get(layer.instance) else { continue };
        let project_index = packages
            .get(instance.package())
            .and_then(|p| p.light_layers.get(layer.index))
            .map(|l| l.layer_index);
        let selected = project_index.is_some() && project_index == session.selected;
        let position = global.translation();
        let lit = state.shining() > 0.0;
        let base = light.color.to_linear();
        let alpha = match (selected, lit) {
            (true, true) => 1.0,
            (true, false) => 0.5,
            (false, true) => 0.6,
            (false, false) => 0.25,
        };
        let color = Color::linear_rgba(base.red.max(0.15), base.green.max(0.15), base.blue.max(0.15), alpha);
        let s = 0.08;
        for axis in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 1.0, 0.0).normalize(), Vec3::new(-1.0, 1.0, 0.0).normalize()] {
            gizmos.line(position - axis * s, position + axis * s, color);
        }
        let range = state.range.max(light.range);
        if range > 0.0 {
            gizmos.circle(Isometry3d::new(position, Quat::from_rotation_arc(Vec3::Z, Vec3::Y)), range, color.with_alpha(alpha * 0.6));
            if selected {
                gizmos.sphere(Isometry3d::from_translation(position), range, color.with_alpha(0.25)).resolution(24);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_make_valid_lights() {
        let mut triggers = Vec::new();
        let param = ParamDef { name: "charge".into(), ..Default::default() };
        let sockets = vec![SocketDef { name: "hand".into(), ..Default::default() }];
        let mut layer = new_light_layer(&mut triggers, &sockets, &["light".to_string()], Some(&param));
        assert_eq!(layer.name, "light2");
        assert_eq!(layer.attach, AttachDef::Socket("hand".into()));
        assert_eq!(layer.intensity.param(), Some("charge"));
        assert!(!layer.shadows);
        for preset in LightPreset::ALL {
            apply_light_preset(&mut layer, preset, &mut triggers, None);
            assert_eq!(layer.name, "light2", "{:?} keeps the name", preset);
            assert_eq!(layer.attach, AttachDef::Socket("hand".into()), "{:?} keeps the attach", preset);
            let v = layer.values_at(&|_| None, 0.0);
            assert!(v.intensity > 0.0 && v.range > 0.0, "{:?}: {:?}", preset, v);
            if let PlayDef::OnTrigger(t) = &layer.play {
                assert_eq!(t, HIT);
                assert!(layer.is_one_shot());
            }
        }
        assert_eq!(triggers.len(), 1, "the hit trigger was added once");
    }
}
