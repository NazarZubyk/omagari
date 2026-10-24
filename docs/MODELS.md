# Previewing effects on characters and weapons

Most effects in a game sit on a character or a weapon: a skin that covers the body, sparks on a blade, a glow in the hand. The editor's **MODEL** stage plays the effect on your own glTF model, animated, so you can place sockets on its bones and judge the effect on the real thing.

Omagari ships **no models**. Use your game's own characters, or download a test character yourself (see [Getting a test character](#getting-a-test-character)).

- [What you need](#what-you-need)
- [Showing a model](#showing-a-model)
- [Animations](#animations)
- [Sockets on bones](#sockets-on-bones)
- [Weapons](#weapons)
- [Checks against the model](#checks-against-the-model)
- [Getting a test character](#getting-a-test-character)
- [The demos and their models](#the-demos-and-their-models)

## What you need

- A **glTF** model (`.glb`, or `.gltf` with its files) inside your **game assets folder** (see [GETTING_STARTED.md](GETTING_STARTED.md#3-point-omagari-at-your-game)). The editor loads models from there only, with the same paths your game uses.
- For animation: the model's own clips, or clips in other glTF files with the **same skeleton** (the same bone names).
- No model at all? The **CAPSULE** stage is a 0.7 × 1.8 m capsule whose centre is the target. Surface skins, afterimages, shapes and sockets that are plain offsets all work on it.

## Showing a model

**PREVIEW** tab → **Model**:

| Setting | Meaning |
| :-- | :-- |
| **Model file** | A `.glb` / `.gltf` of the game assets folder. Picking one sets the MODEL stage. |
| **Root height** | How far above the model's feet your game's entity root is, in metres. The model stands this much below the target. Buttons: `0.9 (player root)` for a root at the centre of a 1.8 m character, `0 (feet)` for a root at the feet. |
| **Animation file** / **Clip** | Where the clip comes from (the model's file, or another file) and which clip. |
| **Speed** | Animation speed. The bottom bar has pause, loop, a time slider to scrub and speed. |
| **In place** | Removes the clip's horizontal root motion (the skeleton root, for example Mixamo's hips, walking away from the model's origin), so the character stays on the target while you tune; the vertical bob stays. Preview only: your game handles root motion itself. On by default. |
| **Instance on** | Whether the effect is attached to the **body** or the **weapon** model. |
| **Attach to** | A node of that model the effect instance is attached to (your game's target entity), or the model's root. |
| **Weapon** | A second model held by a node of the body (see [Weapons](#weapons)). |

With **Target motion** (same tab) the model runs along a circle or back and forth, facing where it goes (glTF +Z is forward), so you see how `Local` and `Global` layers and trails behave on a moving character.

If the model file is missing or does not load, the MODEL stage shows the capsule. The PREVIEW tab and the status line say why.

Everything here is saved in the project's `preview` block. It is for the editor only and is **not** part of the exported package.

## Animations

- A clip plays on nodes with the **same names**; there is no retargeting. A clip from another file works when that file's skeleton uses the same bone names.
- **Mixamo** downloads every animation as a separate file whose clip is called `mixamo.com`: pick the file in **Animation file** and the clip `mixamo.com`.
- **Fire at clip time** (TRIGGERS tab) fires a trigger each time the animation passes a moment, for example the hit frame of a swing. Trail and afterimage layers have a **Swing preview** that fires their start and stop triggers at two clip times.

## Sockets on bones

A **socket** is a named point on the target that layers attach to: a node (bone) name plus an offset in metres, or just an offset from the target's origin.

- **🌲 HIERARCHY** tab: the model's nodes, bones, meshes and materials as a searchable tree. **⚓+** on a bone adds a socket on it; **A** / **B** make it the selected layer's segment start / end; **🎯** attaches the preview instance to it.
- **⚓ SOCKETS & FIT** tab: the node picker lists the model's nodes; type the offset, use the nudge buttons, or drag the socket's dot in the viewport.
- A **segment** (two sockets) is a line: sparks along a blade, a trail from a blade's base to its tip, heat haze along it. Two sockets on the hand bone with different offsets make a "virtual blade" without any weapon file.

In a game, a socket's node is found by `Name` below the target (glTF scenes name every node), so the same package works on every character with that bone name. A socket whose node is missing sits at the target's origin, in the editor (labelled `(node not found)`) and in a game.

## Weapons

**PREVIEW → Weapon** holds a second model in a node of the body (for example `mixamorig:RightHand`) with an offset (metres), rotation (degrees) and scale.

- The weapon is a **separate model**: effects on the body (surface skins, the height fit) do not cover it. In a game this is a glTF scene spawned under a bone, or an entity marked `VfxSeparateModel`.
- To make a **weapon effect**, set **Instance on: weapon**: the instance is then attached to the weapon, like your game's `spawn_vfx(&package, weapon_entity)`, and the weapon's sockets are found below it.

## Checks against the model

With a preview model, the checks (the problems counter, and Claude's `check_project`) know its nodes:

- socket nodes, attach names, **Attach to** and the weapon node that the model does not have are **errors**;
- a clip the animation file does not have is an error; clip nodes the model lacks are a warning;
- surface and afterimage targets (mesh and material names) are checked against the model;
- a missing model or animation file is a **warning** (the editor shows the capsule, and sockets are not checked).

Without a preview model, socket names that are not package sockets are **notes**: they must exist as node names in your game's model.

## Getting a test character

If your game has no character yet:

- **Mixamo** (Adobe, free with an account) has rigged humanoid characters and thousands of animations. Download a character (for example "Y Bot") and a few animations yourself. Mixamo exports FBX: convert it to glTF (`.glb`), for example by importing it into Blender and exporting glTF 2.0. Export animations "without skin" to keep their files small; they share the character's `mixamorig:` bone names.
- **Any rigged glTF model** works: your own, one from an asset store, or the Khronos glTF sample models with skins and animations.

**Licensing.** Check the license of every model and animation. Keep them in your game's folder: do not commit them to the Omagari repository, which ships no third-party models.

Put the files under your game assets folder, for example `characters/hero.glb` and `characters/run.glb`, then pick them in the PREVIEW tab.

## The demos and their models

Most demos in `saved_effects/` were made on a Mixamo "Y Bot" (a 1.8 m humanoid) with these files in the game assets folder. None of them are shipped:

| File | What it is |
| :-- | :-- |
| `anim/tpose.glb` | The character (skinned, T-pose) |
| `anim/run.glb` | A running clip (`mixamo.com`) |
| `anim/BreathingIdle.glb` | An idle clip |
| `anim/StableSwordOutwardSlash.glb` | A sword slash clip |

Without these files the demos still open and play: the MODEL stage shows the capsule, the status line says the preview model is missing, and sockets on bones sit at the capsule's centre (plus their offsets). Clip-timed triggers (a swing's start and end) do not fire without the animation; fire them yourself in the TRIGGERS tab.

| Demo | Model and clip | Sockets on bones | Without the model |
| :-- | :-- | :-- | :-- |
| `energy_shell`, `frost`, `status_skin` | character, run | none (surface over all meshes) | Fine on the capsule. |
| `shield_bubble`, `ground_ring`, `shockwave`, `slash_arc` | character, run | none | Fine on the capsule. |
| `mesh_debris_demo` | character, run | none | Fine. |
| `hit_flash`, `shockwave_ripple` | character, idle | none | Fine. |
| `ribbon_demo` | character, run | `hand` on `mixamorig:RightHand` | The hand streak starts at the capsule's centre. |
| `weapon_glow` | character, idle | `hand` on `mixamorig:RightHand` | The glow sits at the capsule's centre. |
| `model_demo` | character, run | `hand`, `blade_base`, `blade_tip` on `mixamorig:RightHand` | Hand effects at the centre; the swing burst needs the clip (fire `swing` yourself). |
| `blade_haze` | character, idle | `blade_base`, `blade_tip` on `mixamorig:RightHand` | The haze is a short capsule at the centre. |
| `swing_sparks`, `sword_trail` | character, sword slash | `blade_base`, `blade_tip` on `mixamorig:RightHand` | The blade does not move: fire `swing_start` and turn on Target motion to see the trail. `blade_swing` is the same trail without a model. |

To use them with **your** character: pick your model and clips in the PREVIEW tab and, if its bones are not named like Mixamo's, change the sockets' node to your hand bone in the SOCKETS & FIT tab. Every demo uses a root height of 0.9 m (the target at the centre of a 1.8 m body).

Demos that need no model at all: `blade_swing`, `attach_demo`, `hanabi_coverage_demo`, and the particle examples in `examples/`.
