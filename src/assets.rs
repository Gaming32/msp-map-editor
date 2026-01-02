use bevy::asset::embedded_asset;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::scene::SceneInstanceReady;
use std::f32::consts::PI;

macro_rules! asset_path {
    ($path:literal) => {
        concat!("embedded://msp_map_editor/assets/", $path)
    };
}

pub struct EmbeddedAssetsPlugin;

impl Plugin for EmbeddedAssetsPlugin {
    fn build(&self, app: &mut App) {
        // app.add_plugins(FbxPlugin);
        app.add_observer(change_materials);

        embedded_asset!(app, "assets/objects/gold_pipe.glb");
        embedded_asset!(app, "assets/objects/key_gate.glb");
        embedded_asset!(app, "assets/objects/star.glb");
    }
}

pub fn spawn_gold_pipe<'a>(
    commands: &'a mut Commands,
    world: &World,
    position: Vec3,
) -> EntityCommands<'a> {
    commands.spawn((
        SceneRoot(world.load_asset(asset_path!("objects/gold_pipe.glb#Scene0"))),
        MaterialChangeType::GoldPipe,
        NotShadowCaster,
        Transform::from_translation(position)
            .with_scale(Vec3::splat(0.375))
            .with_rotation(Quat::from_rotation_y(PI / 2.0)),
    ))
}

pub fn spawn_key_gate<'a>(
    commands: &'a mut Commands,
    world: &World,
    position: Vec3,
    rotation: f32,
) -> EntityCommands<'a> {
    commands.spawn((
        SceneRoot(world.load_asset(asset_path!("objects/key_gate.glb#Scene0"))),
        MaterialChangeType::KeyGate,
        Transform::from_translation(position)
            .with_scale(Vec3::splat(10.0 / 16.0))
            .with_rotation(Quat::from_rotation_y(rotation)),
    ))
}

pub fn spawn_star<'a>(
    commands: &'a mut Commands,
    world: &World,
    position: Vec3,
) -> EntityCommands<'a> {
    commands.spawn((
        SceneRoot(world.load_asset(asset_path!("objects/star.glb#Scene0"))),
        MaterialChangeType::Star,
        Transform::from_translation(position).with_scale(Vec3::splat(0.15)),
    ))
}

pub fn spawn_silver_star<'a>(
    commands: &'a mut Commands,
    world: &World,
    position: Vec3,
) -> EntityCommands<'a> {
    commands.spawn((
        SceneRoot(world.load_asset(asset_path!("objects/star.glb#Scene0"))),
        MaterialChangeType::SilverStar,
        Transform::from_translation(position).with_scale(Vec3::splat(0.1)),
    ))
}

#[derive(Component, Copy, Clone, Debug)]
enum MaterialChangeType {
    GoldPipe,
    KeyGate,
    Star,
    SilverStar,
}

fn change_materials(
    scene_ready: On<SceneInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    change_type: Query<&MaterialChangeType>,
    mesh_materials: Query<&MeshMaterial3d<StandardMaterial>>,
    mut asset_materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(change_type) = change_type.get(scene_ready.entity) else {
        return;
    };
    for descendant in children.iter_descendants(scene_ready.entity) {
        let Ok(id) = mesh_materials.get(descendant) else {
            continue;
        };
        let Some(material) = asset_materials.get(id.id()) else {
            continue;
        };

        let new_material = match change_type {
            MaterialChangeType::GoldPipe => StandardMaterial {
                base_color_texture: material.base_color_texture.clone(),
                perceptual_roughness: 1.0,
                ..Default::default()
            },
            MaterialChangeType::KeyGate => StandardMaterial {
                alpha_mode: AlphaMode::Opaque,
                ..material.clone()
            },
            MaterialChangeType::Star | MaterialChangeType::SilverStar
                if material.base_color_texture.is_some() =>
            {
                StandardMaterial {
                    base_color_texture: material.base_color_texture.clone(),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 1.0,
                    ..Default::default()
                }
            }
            MaterialChangeType::Star => StandardMaterial {
                base_color: Srgba::rgb_u8(0xFF, 0xFF, 0x00).into(),
                perceptual_roughness: 0.075,
                metallic: 0.8,
                ..Default::default()
            },
            MaterialChangeType::SilverStar => StandardMaterial {
                base_color: Srgba::rgb_u8(0xAA, 0xAA, 0xAA).into(),
                perceptual_roughness: 0.075,
                metallic: 0.8,
                ..Default::default()
            },
        };
        commands
            .entity(descendant)
            .insert(MeshMaterial3d(asset_materials.add(new_material)));
    }
}
