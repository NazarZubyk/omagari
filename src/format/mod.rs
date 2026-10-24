//! The Omagari project format: the serializable effect data model and
//! `produce()`, which builds bevy_hanabi [`EffectAsset`]s from it.
//!
//! - [`ProjectV2`]: the current format (`*.omagari.ron` projects and
//!   `*.vfx.ron` packages). [`ProjectV2::from_ron_str`] also reads v1 files and
//!   migrates them ([`migrate_v1`]).
//! - [`OmagariProject`]: the v1 format, still used by the editor and MCP until
//!   they switch to v2.
//!
//! This module has no UI code. The editor's egui widgets for these types live
//! in the `omagari` binary, so games can load and build projects with the
//! library alone.

use bevy::prelude::*;
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};

use crate::{ExportedEffect, ExportedProject};

pub mod coverage;
mod expr;
pub mod hanabi_serde;
mod light;
mod migrate;
mod modifiers;
mod particles;
mod project;
mod shape;
mod surface;
mod trail;
pub mod unknown_fields;

pub use expr::*;
pub use hanabi_serde::{CpuValueDef, CpuVecDef, SpawnerDef, SpawnerPreset};
pub use light::*;
pub use migrate::*;
pub use modifiers::*;
pub use particles::*;
pub use project::*;
pub use shape::*;
pub use surface::*;
pub use trail::*;

/// A v1 editor project: a list of particle effects, saved as `*.omagari.ron`.
#[derive(Resource, Serialize, Deserialize, Default, Clone)]
pub struct OmagariProject {
    pub effects: Vec<EffectEditor>,
}

impl OmagariProject {
    /// Parses a project from `*.omagari.ron` text.
    pub fn from_ron_str(text: &str) -> ron::error::SpannedResult<Self> {
        ron::from_str(text)
    }

    /// Serializes the project the way the editor saves it.
    pub fn to_ron_string(&self) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(self, PrettyConfig::new().new_line("\n".to_string()))
    }

    /// Builds every effect, ready to be written as a `*.hanabi.ron` export.
    pub fn to_exported(&self) -> ExportedProject {
        let mut to_export = ExportedProject::default();
        for effect in self.effects.iter() {
            to_export.effects.push(ExportedEffect {
                name: effect.name().to_string(),
                parent: effect.parent(),
                texture_index: effect.texture_index(),
                effect_asset: effect.produce(),
            });
        }
        to_export
    }
}
