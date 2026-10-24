//! Per-layer visibility and solo for the preview. Editor-only (not saved):
//! hidden layers keep simulating (so their child layers still spawn) but
//! draw nothing.

use std::collections::HashSet;

use omagari::format::{AlphaModeDef, LayerDef, ProjectV2};

/// Alpha cutoff above any possible alpha: a `Mask` layer with it discards
/// every pixel.
const HIDDEN_CUTOFF: f32 = 2.0;

#[derive(Default, Clone, Debug, PartialEq)]
pub struct LayerView {
    hidden: HashSet<String>,
    solo: HashSet<String>,
}

impl LayerView {
    pub fn is_hidden(&self, name: &str) -> bool {
        self.hidden.contains(name)
    }

    pub fn is_solo(&self, name: &str) -> bool {
        self.solo.contains(name)
    }

    pub fn toggle_hidden(&mut self, name: &str) {
        if !self.hidden.remove(name) {
            self.hidden.insert(name.to_string());
        }
    }

    pub fn toggle_solo(&mut self, name: &str) {
        if !self.solo.remove(name) {
            self.solo.insert(name.to_string());
        }
    }

    /// Whether the layer draws in the preview: with any layer soloed only the
    /// soloed ones, else every layer that is not hidden.
    pub fn shows(&self, name: &str) -> bool {
        if self.solo.is_empty() {
            !self.hidden.contains(name)
        } else {
            self.solo.contains(name)
        }
    }

    /// Forgets layers that no longer exist (deleted or renamed).
    pub fn retain_layers(&mut self, project: &ProjectV2) {
        let exists = |name: &String| project.layers.iter().any(|l| l.name() == name);
        self.hidden.retain(exists);
        self.solo.retain(exists);
    }

    /// Whether anything is hidden or soloed.
    pub fn is_filtering(&self) -> bool {
        !self.hidden.is_empty() || !self.solo.is_empty()
    }

    /// The project the preview builds: particle layers that don't show get an
    /// alpha mask nothing passes, so they still simulate and spawn children;
    /// surface layers that don't show lose their shells, shapes, trails and
    /// afterimages become fully transparent, lights dark, distortions flat.
    pub fn apply(&self, project: &ProjectV2) -> ProjectV2 {
        let mut project = project.clone();
        for layer in project.layers.iter_mut() {
            let shows = self.shows(layer.name());
            match layer {
                LayerDef::Particles(p) if !shows => p.effect.alpha_mode = AlphaModeDef::Mask(HIDDEN_CUTOFF),
                LayerDef::Particles(_) => {}
                // A hidden surface layer draws no overlays.
                LayerDef::Surface(s) if !shows => s.shells.clear(),
                LayerDef::Surface(_) => {}
                // A hidden shape draws nothing (fully transparent).
                LayerDef::Shape(s) if !shows => {
                    s.material.opacity = omagari::format::Binding::Const(0.0);
                    s.material.rim = None;
                    s.material.flash = None;
                    s.material.dissolve = None;
                }
                LayerDef::Shape(_) => {}
                LayerDef::Trail(t) if !shows => hide_material(&mut t.material),
                LayerDef::Afterimage(a) if !shows => hide_material(&mut a.material),
                LayerDef::Trail(_) | LayerDef::Afterimage(_) => {}
                // A hidden light is dark.
                LayerDef::Light(l) if !shows => l.intensity = omagari::format::Binding::Const(0.0),
                LayerDef::Light(_) => {}
                // A hidden distortion bends nothing (and is not drawn).
                LayerDef::Distortion(d) if !shows => {
                    d.strength = omagari::format::Binding::Const(0.0);
                    d.tint = omagari::format::Binding::Const(bevy::prelude::Vec4::new(1.0, 1.0, 1.0, 0.0));
                }
                LayerDef::Distortion(_) => {}
            }
        }
        project
    }
}

/// Makes an effect material fully transparent (a hidden layer).
fn hide_material(m: &mut omagari::format::EffectMaterialDef) {
    m.opacity = omagari::format::Binding::Const(0.0);
    m.rim = None;
    m.flash = None;
    m.dissolve = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use omagari::format::ParticlesLayerDef;

    fn project(names: &[&str]) -> ProjectV2 {
        let mut project = ProjectV2::default();
        for name in names {
            project.layers.push(LayerDef::Particles(ParticlesLayerDef {
                name: name.to_string(),
                ..Default::default()
            }));
        }
        project
    }

    fn alpha(project: &ProjectV2, index: usize) -> AlphaModeDef {
        project.layers[index].as_particles().unwrap().effect.alpha_mode
    }

    #[test]
    fn hidden_layers_are_masked_out_and_others_untouched() {
        let p = project(&["a", "b"]);
        let mut view = LayerView::default();
        view.toggle_hidden("a");
        let built = view.apply(&p);
        assert_eq!(alpha(&built, 0), AlphaModeDef::Mask(HIDDEN_CUTOFF));
        assert_eq!(alpha(&built, 1), alpha(&p, 1));
        assert_eq!(built.layers.len(), 2, "hidden layers still simulate");
        view.toggle_hidden("a");
        assert!(!view.is_filtering());
    }

    #[test]
    fn solo_wins_over_hidden() {
        let p = project(&["a", "b", "c"]);
        let mut view = LayerView::default();
        view.toggle_hidden("b");
        view.toggle_solo("b");
        view.toggle_solo("c");
        assert!(!view.shows("a"));
        assert!(view.shows("b"));
        assert!(view.shows("c"));
        let built = view.apply(&p);
        assert_eq!(alpha(&built, 0), AlphaModeDef::Mask(HIDDEN_CUTOFF));
        assert_eq!(alpha(&built, 1), alpha(&p, 1));
    }

    #[test]
    fn removed_layers_are_forgotten() {
        let mut view = LayerView::default();
        view.toggle_solo("gone");
        view.toggle_hidden("a");
        view.retain_layers(&project(&["a"]));
        assert!(!view.is_solo("gone"));
        assert!(view.is_hidden("a"));
        assert!(!view.shows("a"));
    }
}
