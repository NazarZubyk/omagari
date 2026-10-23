use bevy::{
    asset::AssetLoader,
    platform::collections::HashMap,
    prelude::*,
    reflect::{TypeRegistry, TypeRegistryArc},
};
use bevy_hanabi::prelude::*;
use bevy_hanabi::{register_reflect_modifier, EffectAssetDeserializer, EffectAssetSerializer};
use serde::{
    de::{DeserializeSeed, MapAccess, SeqAccess, Visitor},
    ser::{SerializeSeq, SerializeStruct},
    Deserialize, Serialize, Serializer,
};
use std::{
    fmt,
    io::{self},
    time::Duration,
};

pub mod format;
pub mod runtime;

pub use runtime::{VfxCommandsExt, VfxControl, VfxFinished, VfxInstance, VfxPackage};

/// The Omagari runtime plugin. Add it after `DefaultPlugins` and `HanabiPlugin`.
///
/// - VFX packages: loads `*.vfx.ron` / `*.omagari.ron` as [`VfxPackage`]s and
///   plays them with [`VfxCommandsExt::spawn_vfx`] / [`VfxControl`].
/// - Built-in textures (`omagari://textures/<file>`) embedded in the library.
/// - Legacy: [`EffectComplex`] (`*.hanabi.ron` exports) and its despawn timer.
pub struct OmagariPlugin;

impl Plugin for OmagariPlugin {
    fn build(&self, app: &mut App) {
        register_missing_hanabi_modifiers(app.world().resource::<AppTypeRegistry>());
        app.add_systems(Update, despawn_effects_on_timer)
           .init_asset::<EffectComplex>()
           .init_asset_loader::<OmagariAssetLoader>();
        runtime::build(app);
    }
}

/// Registers Hanabi modifiers that bevy_hanabi 0.19's own `register_modifiers()`
/// leaves out, so effects using them can be deserialized.
///
/// Currently this is only `EmitSpawnEventModifier`, which parent effects use to
/// spawn particles in child effects. Without it, loading an exported file with a
/// parent/child effect fails with "no ReflectModifier type data".
pub fn register_missing_hanabi_modifiers(type_registry: &AppTypeRegistry) {
    type_registry.write().register::<EmitSpawnEventModifier>();
    register_reflect_modifier::<EmitSpawnEventModifier>(type_registry, |module| {
        Box::new(EmitSpawnEventModifier {
            condition: EventEmitCondition::Always,
            count: module.lit(1u32),
            child_index: 0,
        })
    });
}

/// EffectComplex represents a collection of prepared effects in the application.
#[derive(Asset, TypePath, Debug)]
pub struct EffectComplex {
    prepared_effects: Vec<PreparedEffect>,
}

impl EffectComplex {
    /// Spawns effects based on the configuration stored in EffectComplex.
    ///
    /// # Parameters
    /// - `commands`: Mutable reference to Commands used to spawn entities.
    /// - `textures`: Vector of image handles used to apply textures to the effects.
    /// - `pos`: Position to spawn the effects at.
    /// - `despawn_in`: Optional duration for when to despawn the effects.
    pub fn spawn(
        &self,
        commands: &mut Commands,
        textures: &Vec<Handle<Image>>,
        pos: Vec3,
        despawn_in: Option<Duration>,
    ) {
        let mut refs: HashMap<String, Entity> = HashMap::new();
        for prepared_effect in self.prepared_effects.iter() {
            let mut e = commands.spawn((
                Name::new(prepared_effect.name.clone()),
                ParticleEffect::new(prepared_effect.effect_handle.clone()),
                Transform::from_translation(pos),
            ));

            if let Some(texture_index) = prepared_effect.texture_index {
                e.insert(EffectMaterial {
                    images: vec![textures[texture_index].clone()],
                });
            }

            if let Some(despawn_in) = despawn_in {
                e.insert(EffectDespawnTimer(Timer::from_seconds(
                    despawn_in.as_secs_f32(),
                    TimerMode::Once,
                )));
            }

            refs.insert(prepared_effect.name.clone(), e.id());

            if let Some(parent) = &prepared_effect.parent {
                if let Some(entity) = refs.get(parent) {
                    e.insert(EffectParent::new(*entity));
                } else {
                    // TODO: Raise an error
                }
            }
        }
    }

    /// Creates an EffectComplex from raw byte data.
    ///
    /// # Parameters
    /// - `bytes`: Vector of bytes representing the effect configuration.
    /// - `load_context`: Mutable reference to LoadContext for managing assets during loading.
    /// - `type_registry`: Type registry holding every Hanabi modifier type referenced by the file.
    ///
    /// # Returns
    /// - Result containing either the EffectComplex or an IO error.
    fn from_bytes(
        bytes: Vec<u8>,
        load_context: &mut bevy::asset::LoadContext<'_>,
        type_registry: &TypeRegistry,
    ) -> Result<Self, io::Error> {
        let omagari_project = ExportedProject::from_ron_bytes(&bytes, type_registry)?;

        let mut prepared_effects: Vec<PreparedEffect> = Vec::new();
        for (index, effect) in omagari_project.effects.iter().enumerate() {
            // Each effect needs its own label, otherwise they overwrite each other.
            let h = load_context.add_labeled_asset(
                format!("EffectAsset{}", index),
                effect.effect_asset.clone(),
            );
            prepared_effects.push(PreparedEffect {
                name: effect.name.to_string(),
                parent: effect.parent.clone(),
                texture_index: effect.texture_index.clone(),
                effect_handle: h.clone(),
            });
        }
        Ok(Self { prepared_effects })
    }
}

/// System function that despawns effects whose timers have expired.
///
/// # Parameters
/// - `commands`: Commands used to remove entities.
/// - `effects_with_timer`: Query for entities with an EffectDespawnTimer component.
/// - `time`: A reference to the current time resource.
fn despawn_effects_on_timer(
    mut commands: Commands,
    mut effects_with_timer: Query<(Entity, &mut EffectDespawnTimer)>,
    time: Res<Time>,
) {
    for (entity, mut timer) in effects_with_timer.iter_mut() {
        timer.0.tick(time.delta());
        if timer.0.is_finished() {
            // TODO: Despawn more gracefully by stopping emissions first
            commands.entity(entity).try_despawn();
        }
    }
}

/// Struct representing an exported effect.
///
/// Since bevy_hanabi 0.19, [`EffectAsset`] can only be (de)serialized together
/// with a [`TypeRegistry`], so this type has no plain serde derives. Use
/// [`ExportedProject::to_ron_string`] and [`ExportedProject::from_ron_bytes`].
#[derive(Resource, Default)]
pub struct ExportedEffect {
    pub name: String,
    pub parent: Option<String>,
    pub texture_index: Option<usize>,
    pub effect_asset: EffectAsset,
}

/// Struct representing an exported project containing multiple effects.
#[derive(Resource, Default)]
pub struct ExportedProject {
    pub effects: Vec<ExportedEffect>,
}

impl ExportedProject {
    /// Serializes the project to a pretty RON string (the `*.hanabi.ron` format).
    ///
    /// The `type_registry` must contain every modifier type used by the effects;
    /// the app's `AppTypeRegistry` does once `HanabiPlugin` is added.
    pub fn to_ron_string(&self, type_registry: &TypeRegistry) -> Result<String, ron::Error> {
        ron::ser::to_string_pretty(
            &ProjectSer {
                project: self,
                type_registry,
            },
            ron::ser::PrettyConfig::new().new_line("\n".to_string()),
        )
    }

    /// Deserializes a project from RON bytes (the `*.hanabi.ron` format).
    pub fn from_ron_bytes(bytes: &[u8], type_registry: &TypeRegistry) -> Result<Self, io::Error> {
        let invalid = |e: ron::error::SpannedError| io::Error::new(io::ErrorKind::InvalidData, e);
        let mut deserializer = ron::de::Deserializer::from_bytes(bytes).map_err(invalid)?;
        let project = ProjectSeed { type_registry }
            .deserialize(&mut deserializer)
            .map_err(|e| invalid(deserializer.span_error(e)))?;
        deserializer
            .end()
            .map_err(|e| invalid(deserializer.span_error(e)))?;
        Ok(project)
    }
}

struct ProjectSer<'a> {
    project: &'a ExportedProject,
    type_registry: &'a TypeRegistry,
}

impl Serialize for ProjectSer<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("ExportedProject", 1)?;
        s.serialize_field(
            "effects",
            &EffectsSer {
                effects: &self.project.effects,
                type_registry: self.type_registry,
            },
        )?;
        s.end()
    }
}

struct EffectsSer<'a> {
    effects: &'a [ExportedEffect],
    type_registry: &'a TypeRegistry,
}

impl Serialize for EffectsSer<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.effects.len()))?;
        for effect in self.effects {
            seq.serialize_element(&EffectSer {
                effect,
                type_registry: self.type_registry,
            })?;
        }
        seq.end()
    }
}

struct EffectSer<'a> {
    effect: &'a ExportedEffect,
    type_registry: &'a TypeRegistry,
}

impl Serialize for EffectSer<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("ExportedEffect", 4)?;
        s.serialize_field("name", &self.effect.name)?;
        s.serialize_field("parent", &self.effect.parent)?;
        s.serialize_field("texture_index", &self.effect.texture_index)?;
        s.serialize_field(
            "effect_asset",
            &EffectAssetSerializer::new(&self.effect.effect_asset, self.type_registry),
        )?;
        s.end()
    }
}

struct ProjectSeed<'a> {
    type_registry: &'a TypeRegistry,
}

impl<'de> DeserializeSeed<'de> for ProjectSeed<'_> {
    type Value = ExportedProject;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Effects,
        }

        struct ProjectVisitor<'a>(&'a TypeRegistry);

        impl<'de> Visitor<'de> for ProjectVisitor<'_> {
            type Value = ExportedProject;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct ExportedProject")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut effects = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Effects => effects = Some(map.next_value_seed(EffectsSeed(self.0))?),
                    }
                }
                Ok(ExportedProject {
                    effects: effects.ok_or_else(|| serde::de::Error::missing_field("effects"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "ExportedProject",
            &["effects"],
            ProjectVisitor(self.type_registry),
        )
    }
}

struct EffectsSeed<'a>(&'a TypeRegistry);

impl<'de> DeserializeSeed<'de> for EffectsSeed<'_> {
    type Value = Vec<ExportedEffect>;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        struct EffectsVisitor<'a>(&'a TypeRegistry);

        impl<'de> Visitor<'de> for EffectsVisitor<'_> {
            type Value = Vec<ExportedEffect>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a list of ExportedEffect")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut effects = Vec::new();
                while let Some(effect) = seq.next_element_seed(EffectSeed(self.0))? {
                    effects.push(effect);
                }
                Ok(effects)
            }
        }

        deserializer.deserialize_seq(EffectsVisitor(self.0))
    }
}

struct EffectSeed<'a>(&'a TypeRegistry);

impl<'de> DeserializeSeed<'de> for EffectSeed<'_> {
    type Value = ExportedEffect;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Name,
            Parent,
            TextureIndex,
            EffectAsset,
        }

        struct EffectVisitor<'a>(&'a TypeRegistry);

        impl<'de> Visitor<'de> for EffectVisitor<'_> {
            type Value = ExportedEffect;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct ExportedEffect")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut name = None;
                let mut parent = None;
                let mut texture_index = None;
                let mut effect_asset = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Name => name = Some(map.next_value()?),
                        Field::Parent => parent = Some(map.next_value()?),
                        Field::TextureIndex => texture_index = Some(map.next_value()?),
                        Field::EffectAsset => {
                            effect_asset =
                                Some(map.next_value_seed(EffectAssetDeserializer::new(self.0))?)
                        }
                    }
                }
                Ok(ExportedEffect {
                    name: name.ok_or_else(|| serde::de::Error::missing_field("name"))?,
                    parent: parent.unwrap_or_default(),
                    texture_index: texture_index.unwrap_or_default(),
                    effect_asset: effect_asset
                        .ok_or_else(|| serde::de::Error::missing_field("effect_asset"))?,
                })
            }
        }

        deserializer.deserialize_struct(
            "ExportedEffect",
            &["name", "parent", "texture_index", "effect_asset"],
            EffectVisitor(self.0),
        )
    }
}

/// Struct representing a prepared effect with all its dependencies resolved.
#[derive(Debug)]
struct PreparedEffect {
    name: String,
    parent: Option<String>,
    texture_index: Option<usize>,
    effect_handle: Handle<EffectAsset>,
}

/// Component representing a timer for despawning an effect.
#[derive(Component)]
struct EffectDespawnTimer(Timer);

/// Asset loader for Omagari effects.
///
/// Holds the app type registry, which bevy_hanabi needs to deserialize modifiers.
#[derive(TypePath)]
struct OmagariAssetLoader {
    type_registry: TypeRegistryArc,
}

impl FromWorld for OmagariAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            type_registry: world.resource::<AppTypeRegistry>().0.clone(),
        }
    }
}

impl AssetLoader for OmagariAssetLoader {
    type Asset = EffectComplex;
    type Settings = ();
    type Error = std::io::Error;
    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &(),
        mut load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, std::io::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let type_registry = self.type_registry.read();
        Ok(EffectComplex::from_bytes(bytes, &mut load_context, &type_registry)?)
    }
}
