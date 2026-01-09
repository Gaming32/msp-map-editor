use crate::culling::CullIfInside;
use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::PI;
use std::path::{Path, PathBuf};

macro_rules! asset_path {
    ($path:literal) => {
        concat!("embedded://msp_map_editor/assets/", $path)
    };
}

pub struct EmbeddedAssetsPlugin;

impl Plugin for EmbeddedAssetsPlugin {
    fn build(&self, app: &mut App) {
        let registry = app.world().resource::<EmbeddedAssetRegistry>();

        macro_rules! embedded_asset {
            ($path:literal, $real_path:literal) => {
                registry.insert_asset(PathBuf::new(), Path::new($path), include_bytes!($real_path))
            };
        }
        macro_rules! embedded_meta {
            ($path:literal, $real_path:literal) => {
                registry.insert_meta(
                    &PathBuf::new(),
                    Path::new($path),
                    include_bytes!($real_path),
                )
            };
        }

        include!(concat!(env!("OUT_DIR"), "/asset_index.rs"));
    }
}

#[derive(Component)]
pub struct PlayerMarker;

pub fn icons_atlas(assets: &AssetServer) -> Handle<Image> {
    assets.load(asset_path!("icons/icons.png"))
}

pub fn unset_texture_icon(assets: &AssetServer) -> Handle<Image> {
    assets.load(asset_path!("icons/unset_texture.png"))
}

pub fn camera(assets: &AssetServer, position: Vec3, rotation: Quat) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/camera.glb#Scene0"))),
        NotShadowCaster,
        NotShadowReceiver,
        CullIfInside(0.7..1.0),
        Transform::from_translation(position)
            .with_scale(Vec3::splat(0.3))
            .with_rotation(rotation),
    )
}

pub fn gold_pipe(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/gold_pipe.glb#Scene0"))),
        NotShadowCaster,
        Transform::from_translation(position)
            .with_scale(Vec3::splat(0.375))
            .with_rotation(Quat::from_rotation_y(PI / 2.0)),
    )
}

pub fn key_gate(assets: &AssetServer, position: Vec3, rotation: f32) -> (SceneRoot, Transform) {
    (
        SceneRoot(assets.load(asset_path!("objects/key_gate.glb#Scene0"))),
        Transform::from_translation(position)
            .with_scale(Vec3::splat(10.0 / 16.0))
            .with_rotation(Quat::from_rotation_y(rotation)),
    )
}

pub fn star(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/star.glb#Scene0"))),
        Transform::from_translation(position).with_scale(Vec3::splat(0.15)),
    )
}

pub fn floor(assets: &AssetServer) -> Handle<Image> {
    assets.load(asset_path!("floor.png"))
}

pub fn missing_atlas(assets: &AssetServer) -> Handle<Image> {
    assets.load(asset_path!("missing_atlas.png"))
}

pub fn missing_skybox(assets: &AssetServer) -> Handle<Image> {
    assets.load(asset_path!("missing_skybox.ktx2"))
}

pub fn player(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        PlayerMarker,
        Mesh3d(assets.add(Plane3d::new(Vec3::Z, Vec2::new(0.6, 0.75) / 2.0).into())),
        MeshMaterial3d(assets.add(StandardMaterial {
            base_color_texture: Some(assets.load(asset_path!("player.png"))),
            alpha_mode: AlphaMode::Mask(0.5),
            perceptual_roughness: 1.0,
            double_sided: true,
            cull_mode: None,
            ..Default::default()
        })),
        NotShadowReceiver,
        Transform::from_translation(position),
    )
}
