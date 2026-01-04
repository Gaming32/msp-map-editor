use bevy::asset::embedded_path;
use bevy::asset::io::embedded::EmbeddedAssetRegistry;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::PI;
use std::path::PathBuf;

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
            ($app:expr, $path:expr) => {{
                let path = embedded_path!("src", $path);
                registry.insert_asset(PathBuf::new(), &path, include_bytes!($path));
            }};
        }
        macro_rules! embedded_asset_with_meta {
            ($app:expr, $path:expr) => {{
                let path = embedded_path!("src", $path);
                registry.insert_asset(PathBuf::new(), &path, include_bytes!($path));
                registry.insert_meta(
                    &PathBuf::new(),
                    &path,
                    include_bytes!(concat!($path, ".meta")),
                );
            }};
        }

        embedded_asset!(app, "assets/objects/gold_pipe.glb");
        embedded_asset!(app, "assets/objects/key_gate.glb");
        embedded_asset!(app, "assets/objects/star.glb");

        embedded_asset_with_meta!(app, "assets/player.png");
    }
}

#[derive(Component)]
pub struct PlayerMarker;

pub fn gold_pipe(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/gold_pipe.glb#Scene0"))),
        NotShadowCaster,
        Transform::from_translation(position)
            .with_scale(Vec3::splat(0.375))
            .with_rotation(Quat::from_rotation_y(PI / 2.0)),
    )
}

pub fn key_gate(assets: &AssetServer, position: Vec3, rotation: f32) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/key_gate.glb#Scene0"))),
        Transform::from_translation(position)
            .with_scale(Vec3::splat(10.0 / 16.0))
            .with_rotation(Quat::from_rotation_y(rotation)),
    )
}

pub fn silver_star(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/star.glb#Scene0"))),
        Transform::from_translation(position).with_scale(Vec3::splat(0.1)),
    )
}

pub fn star(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        SceneRoot(assets.load(asset_path!("objects/star.glb#Scene0"))),
        Transform::from_translation(position).with_scale(Vec3::splat(0.15)),
    )
}

pub fn player(assets: &AssetServer, position: Vec3) -> impl Bundle {
    (
        PlayerMarker,
        Mesh3d(assets.add(Plane3d::new(Vec3::Z, Vec2::new(0.6, 0.75)).into())),
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

pub fn tutorial_obj(assets: &AssetServer, obj: impl Bundle) -> impl Bundle {
    (
        obj,
        children![(
            Mesh3d(assets.add(Sphere::new(2.0).into())),
            MeshMaterial3d(assets.add(StandardMaterial {
                base_color: Srgba::rgba_u8(161, 61, 204, 32).into(),
                alpha_mode: AlphaMode::Add,
                unlit: true,
                ..Default::default()
            })),
            NotShadowCaster,
            NotShadowReceiver,
        )],
    )
}
