use crate::assets::{
    GoldPipeMarker, PlayerMarker, PodiumMarker, ShopHopBoxMarker, camera, gold_pipe, missing_atlas,
    missing_skybox, player, podium, shop_hop_box,
};
use crate::culling::CullingPlugin;
use crate::load_file::{FileLoaded, LoadedFile};
use crate::mesh::{MapMeshMarker, mesh_map, mesh_top_highlights};
use crate::schema::{MpsVec2, MpsVec2f, MpsVec3};
use crate::sync::{
    CameraId, Direction, EditObject, ListEdit, MapEdit, MapEdited, PresetView, PreviewObject,
    PreviewResultsAnimation, SelectForEditing, TogglePreviewVisibility,
};
use crate::tile_range::TileRange;
use crate::{modifier_key, shortcut_pressed};
use bevy::asset::io::embedded::GetAssetServer;
use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::camera::NormalizedRenderTarget;
use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::camera::visibility::NoFrustumCulling;
use bevy::core_pipeline::Skybox;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseWheel;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::Rect;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDataOrder, TextureDimension, TextureFormat, TextureViewDescriptor,
    TextureViewDimension,
};
use bevy::window::WindowEvent;
use bevy_easings::{CustomComponentEase, EaseFunction, EasingType};
use bevy_map_camera::controller::{CameraControllerButtons, ControlMessage};
use bevy_map_camera::{CameraControllerSettings, LookTransform, MapCamera, MapCameraPlugin};
use bevy_math::bounding::{Aabb3d, BoundingVolume};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbaImage};
use itertools::Itertools;
use std::f32::consts::PI;
use std::time::{Duration, Instant};
use transform_gizmo_bevy::GizmoHotkeys;
use transform_gizmo_bevy::config::TransformPivotPoint;
use transform_gizmo_bevy::prelude::*;

#[derive(Resource)]
pub struct ViewportTarget {
    pub texture: Handle<Image>,
    pub upper_left: Vec2,
    pub size: Vec2,
    pub disable_input: bool,
}

pub struct ViewportPlugin;

impl Plugin for ViewportPlugin {
    fn build(&self, app: &mut App) {
        let render_texture =
            app.world_mut()
                .resource_mut::<Assets<Image>>()
                .add(Image::new_target_texture(
                    1,
                    1,
                    TextureFormat::Rgba8UnormSrgb,
                ));
        let missing_skybox = missing_skybox(app.get_asset_server());
        let missing_atlas = missing_atlas(app.get_asset_server());
        let atlas_material = app
            .world_mut()
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color_texture: Some(missing_atlas.clone()),
                perceptual_roughness: 1.0,
                ..Default::default()
            });

        app.insert_resource(ViewportTarget {
            texture: render_texture,
            upper_left: Vec2::default(),
            size: Vec2::new(1.0, 1.0),
            disable_input: false,
        })
        .insert_resource(ViewportState {
            skybox: ViewportTextureSet::new(missing_skybox),
            atlas: ViewportTextureSet::new(missing_atlas),
            atlas_material,
        })
        .add_plugins((
            MapCameraPlugin,
            MeshPickingPlugin,
            TransformGizmoPlugin,
            CullingPlugin,
        ))
        .add_systems(
            First,
            custom_mouse_pick_events.in_set(PickingSystems::Input),
        )
        .add_systems(Startup, setup_viewport)
        .add_observer(on_file_load)
        .add_observer(on_map_edited)
        .add_observer(on_remesh_map)
        .add_observer(on_select_for_editing)
        .add_observer(on_pointer_click)
        .add_observer(on_preset_view)
        .add_observer(on_toggle_preview_visibility)
        .add_observer(on_preview_results_animation)
        .add_systems(
            Update,
            (
                keyboard_handler,
                ensure_camera_up,
                update_gizmos,
                sync_from_gizmos,
                update_lights,
                update_textures,
                update_results_preview,
                update_results_cameras,
            ),
        );
    }
}

#[derive(Resource)]
struct ViewportState {
    skybox: ViewportTextureSet,
    atlas: ViewportTextureSet,
    atlas_material: Handle<StandardMaterial>,
}

struct ViewportTextureSet {
    missing: Handle<Image>,
    current: Handle<Image>,
    outdated: bool,
}

impl ViewportTextureSet {
    fn new(image: Handle<Image>) -> Self {
        Self {
            missing: image.clone(),
            current: image,
            outdated: false,
        }
    }
}

fn setup_viewport(
    mut commands: Commands,
    viewport_target: Res<ViewportTarget>,
    textures: Res<ViewportState>,
    mut ambient_light: ResMut<AmbientLight>,
    mut gizmos: ResMut<GizmoConfigStore>,
) {
    commands.insert_resource(CameraControllerSettings {
        touch_enabled: false, // XXX: touch pick events are not implemented, so touch wouldn't work anyway. Maybe I should fix this.
        minimum_pitch: 0.0,
        buttons: CameraControllerButtons {
            pan: vec![MouseButton::Middle.into(), KeyCode::ShiftLeft.into()],
            pan_alt: None,
            rotate: vec![MouseButton::Middle.into()],
            rotate_alt: None,
        },
        ..Default::default()
    });
    commands.insert_resource(GizmoOptions {
        pivot_point: TransformPivotPoint::IndividualOrigins,
        snapping: true,
        snap_angle: PI / 4.0,
        snap_distance: 0.5,
        hotkeys: Some(GizmoHotkeys {
            enable_snapping: None,
            ..Default::default()
        }),
        ..Default::default()
    });

    commands.spawn((
        Camera {
            target: viewport_target.texture.clone().into(),
            ..Default::default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 60_f32.to_radians(),
            ..Default::default()
        }),
        Skybox {
            image: textures.skybox.current.clone(),
            brightness: 400.0, // Nits
            rotation: Quat::IDENTITY,
        },
        MapCamera,
        GizmoCamera,
    ));

    commands.spawn((
        LookTransform::default(),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 4000.0,
            ..Default::default()
        },
    ));
    ambient_light.brightness = 160.0;

    gizmos.config_mut::<DefaultGizmoConfigGroup>().0.line.width = 1.0;
}

#[derive(Component)]
struct ViewportObject {
    editor: EditObject,
    old_pos: Vec3,
    old_rot: Option<Quat>,
}

#[derive(Component)]
#[require(Visibility::Visible)]
struct VisibilityToggleable(PreviewObject);
#[derive(Component)]
struct TemporaryViewportObject;
#[derive(Component)]
struct BoundsGizmo(Direction);
#[derive(Component)]
struct TilesGizmo(TileRange);
#[derive(Component)]
struct TilesGizmoMesh(TileRange);
#[derive(Component)]
struct ResultsAnimationPreview(Timer);
#[derive(Component)]
struct ResultsCameraMarker;

#[derive(Component)]
struct AnimationGroupAnchorGizmo;

#[derive(Event)]
struct RemeshMap;

#[allow(clippy::too_many_arguments)]
fn on_file_load(
    _: On<FileLoaded>,
    mut commands: Commands,
    objects: Query<Entity, Or<(With<ViewportObject>, With<MapMeshMarker>)>>,
    camera_query: Query<(&mut LookTransform, &mut Skybox), With<Camera>>,
    mut state: ResMut<ViewportState>,
    assets: Res<AssetServer>,
    file: Res<LoadedFile>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for existing in objects {
        commands.entity(existing).despawn();
    }

    state.skybox.current = state.skybox.missing.clone();
    state.skybox.outdated = true;
    state.atlas.current = file.loaded_textures.atlas.image.clone();
    state.atlas.outdated = false;
    materials
        .get_mut(&state.atlas_material)
        .expect("atlas_material should've been inserted")
        .base_color_texture = Some(state.atlas.current.clone());

    let player_pos = get_player_pos(&file, file.file.starting_tile);
    commands.spawn((
        player(&assets, player_pos),
        ViewportObject {
            editor: EditObject::StartingTile,
            old_pos: player_pos,
            old_rot: None,
        },
    ));
    for (mut camera, mut skybox) in camera_query {
        *camera = get_player_cam_transform(player_pos);
        skybox.image = state.skybox.current.clone();
    }

    for (index, &shop_hop) in file.file.shop_warp_tiles.iter().enumerate() {
        create_shop_hop_box(&mut commands, &file, &assets, shop_hop, index);
    }

    let gold_pipe_pos = get_gold_pipe_pos(&file, file.file.star_warp_tile);
    commands.spawn((
        gold_pipe(&assets, gold_pipe_pos),
        ViewportObject {
            editor: EditObject::StarWarpTile,
            old_pos: gold_pipe_pos,
            old_rot: None,
        },
        VisibilityToggleable(PreviewObject::GoldPipe),
    ));

    let podium_pos = get_podium_pos(&file, file.file.podium_position);
    commands.spawn((
        podium(&assets, podium_pos),
        ViewportObject {
            editor: EditObject::PodiumPosition,
            old_pos: podium_pos,
            old_rot: None,
        },
        Visibility::Hidden,
        VisibilityToggleable(PreviewObject::Podium),
    ));

    for (index, &cam_pos) in file.file.results_anim_cam_poses.iter().enumerate() {
        create_results_camera(&mut commands, &assets, cam_pos, podium_pos + Vec3::Y, index);
    }

    for (transform, id) in [
        (file.file.tutorial_star, CameraId::StarTutorial),
        (file.file.tutorial_shop, CameraId::ShopTutorial),
    ] {
        commands.spawn((
            camera(&assets, transform.pos.into(), transform.rot.into()),
            ViewportObject {
                editor: EditObject::Camera(id),
                old_pos: transform.pos.into(),
                old_rot: Some(transform.rot.into()),
            },
            id,
        ));
    }

    commands.trigger(RemeshMap);
}

fn create_shop_hop_box(
    commands: &mut Commands,
    file: &LoadedFile,
    assets: &AssetServer,
    pos: MpsVec2,
    index: usize,
) {
    let pos = get_shop_hop_pos(file, pos);
    commands.spawn((
        shop_hop_box(assets, pos),
        ViewportObject {
            editor: EditObject::ShopWarpTile(index),
            old_pos: pos,
            old_rot: None,
        },
    ));
}

fn create_results_camera(
    commands: &mut Commands,
    assets: &AssetServer,
    cam_pos: MpsVec3,
    target_pos: Vec3,
    index: usize,
) {
    let cam_pos = Vec3::from(cam_pos) + target_pos;
    commands.spawn((
        camera(assets, cam_pos, Quat::IDENTITY),
        ViewportObject {
            editor: EditObject::ResultsCamera(index),
            old_pos: cam_pos,
            old_rot: None,
        },
        ResultsCameraMarker,
    ));
}

#[allow(clippy::too_many_arguments)]
fn on_map_edited(
    on: On<MapEdited>,
    mut commands: Commands,
    file: Res<LoadedFile>,
    player: Query<
        (&mut Transform, &mut ViewportObject),
        (
            With<PlayerMarker>,
            Without<ShopHopBoxMarker>,
            Without<GoldPipeMarker>,
            Without<PodiumMarker>,
            Without<ResultsCameraMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    mut shop_hop_boxes: Query<
        (Entity, &mut Transform, &mut ViewportObject),
        (
            With<ShopHopBoxMarker>,
            Without<GoldPipeMarker>,
            Without<PodiumMarker>,
            Without<ResultsCameraMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    gold_pipe: Query<
        (&mut Transform, &mut ViewportObject),
        (
            With<GoldPipeMarker>,
            Without<PodiumMarker>,
            Without<ResultsCameraMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    podium: Query<
        (&mut Transform, &mut ViewportObject),
        (
            With<PodiumMarker>,
            Without<ResultsCameraMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    mut results_cameras: Query<
        (Entity, &mut Transform, &mut ViewportObject),
        (
            With<ResultsCameraMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    bounds_markers: Query<
        (&mut Transform, &mut ViewportObject, &BoundsGizmo),
        (
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    mut tiles_gizmo: Query<
        (&mut Transform, &mut ViewportObject, &TilesGizmo),
        (
            Without<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    mut tiles_gizmo_child: Query<
        &mut Transform,
        (
            With<TilesGizmoMesh>,
            Without<CameraId>,
            Without<AnimationGroupAnchorGizmo>,
        ),
    >,
    camera_gizmo: Query<
        (&mut Transform, &mut ViewportObject),
        (With<CameraId>, Without<AnimationGroupAnchorGizmo>),
    >,
    mut anchor_gizmos: Query<
        (Entity, &mut Transform, &mut ViewportObject),
        With<AnimationGroupAnchorGizmo>,
    >,
    mut state: ResMut<ViewportState>,
    assets: Res<AssetServer>,
) {
    let mut change_player_pos = false;
    let mut change_gold_pipe_pos = false;
    let mut change_podium_pos = false;
    let mut change_tiles_gizmos = false;
    let mut change_anchor_gizmos = false;

    match &on.0 {
        MapEdit::StartingTile(_) => {
            change_player_pos = true;
        }
        MapEdit::ShopWarpTile(index, edit) => {
            let mut boxes = shop_hop_boxes
                .iter_mut()
                .sort_by_key::<&ViewportObject, _>(|obj| obj.editor.get_index_param())
                .collect_vec();
            match *edit {
                ListEdit::Set(value) => {
                    let (_, transform, object) = &mut boxes[*index];
                    let pos = get_shop_hop_pos(&file, value);
                    transform.translation = pos;
                    object.old_pos = pos;
                }
                ListEdit::MoveUp => {
                    boxes[*index - 1].2.editor = EditObject::ShopWarpTile(*index);
                    boxes[*index].2.editor = EditObject::ShopWarpTile(*index - 1);
                }
                ListEdit::MoveDown => {
                    boxes[*index].2.editor = EditObject::ShopWarpTile(*index + 1);
                    boxes[*index + 1].2.editor = EditObject::ShopWarpTile(*index);
                }
                ListEdit::Remove => {
                    commands.entity(boxes[*index].0).despawn();
                    for (i, (_, _, object)) in boxes.iter_mut().enumerate().skip(index + 1) {
                        object.editor = EditObject::ShopWarpTile(i - 1);
                    }
                }
                ListEdit::Insert(value) => {
                    for (i, (_, _, object)) in boxes.iter_mut().enumerate().skip(*index) {
                        object.editor = EditObject::ShopWarpTile(i + 1);
                    }
                    create_shop_hop_box(&mut commands, &file, &assets, value, *index);
                }
            }
        }
        MapEdit::StarWarpTile(_) => {
            change_gold_pipe_pos = true;
        }
        MapEdit::PodiumPosition(pos) => {
            change_podium_pos = true;
            let cam_target = get_podium_pos(&file, *pos) + Vec3::Y;
            for (_, mut transform, mut object) in results_cameras {
                let target_pos =
                    Vec3::from(file.file.results_anim_cam_poses[object.editor.get_index_param()])
                        + cam_target;
                transform.translation = target_pos;
                object.old_pos = target_pos;
            }
        }
        MapEdit::ResultsCamera(index, edit) => {
            let cam_target = get_podium_pos(&file, file.file.podium_position) + Vec3::Y;
            let mut cameras = results_cameras
                .iter_mut()
                .sort_by_key::<&ViewportObject, _>(|obj| obj.editor.get_index_param())
                .collect_vec();
            match *edit {
                ListEdit::Set(value) => {
                    let (_, transform, object) = &mut cameras[*index];
                    let pos = Vec3::from(value) + cam_target;
                    transform.translation = pos;
                    object.old_pos = pos;
                }
                ListEdit::MoveUp => {
                    cameras[*index - 1].2.editor = EditObject::ResultsCamera(*index);
                    cameras[*index].2.editor = EditObject::ResultsCamera(*index - 1);
                }
                ListEdit::MoveDown => {
                    cameras[*index].2.editor = EditObject::ResultsCamera(*index + 1);
                    cameras[*index + 1].2.editor = EditObject::ResultsCamera(*index);
                }
                ListEdit::Remove => {
                    commands.entity(cameras[*index].0).despawn();
                    for (i, (_, _, object)) in cameras.iter_mut().enumerate().skip(index + 1) {
                        object.editor = EditObject::ResultsCamera(i - 1);
                    }
                }
                ListEdit::Insert(value) => {
                    for (i, (_, _, object)) in cameras.iter_mut().enumerate().skip(*index) {
                        object.editor = EditObject::ResultsCamera(i + 1);
                    }
                    create_results_camera(&mut commands, &assets, value, cam_target, *index);
                }
            }
        }
        MapEdit::Skybox(_, _) => {
            state.skybox.outdated = true;
        }
        MapEdit::Atlas(_) => {
            state.atlas.outdated = true;
        }
        MapEdit::ExpandMap(_, _) | MapEdit::ShrinkMap(_) => {
            commands.trigger(RemeshMap);
            change_player_pos = true;
            change_gold_pipe_pos = true;
            for (mut transform, mut object, bounds) in bounds_markers {
                transform.translation = get_bounds_gizmo_location(&file, bounds.0);
                object.old_pos = transform.translation;
            }
        }
        MapEdit::ChangeCameraPos(camera, pos) => {
            for (mut transform, mut object) in camera_gizmo {
                if object.editor == EditObject::Camera(*camera) {
                    transform.translation = (*pos).into();
                    object.old_pos = transform.translation;
                }
            }
        }
        MapEdit::ChangeCameraRot(camera, rot) => {
            for (mut transform, mut object) in camera_gizmo {
                if object.editor == EditObject::Camera(*camera) {
                    transform.rotation = (*rot).into();
                    object.old_rot = Some(transform.rotation);
                }
            }
        }
        MapEdit::EditShop(_, _, _) => {}
        MapEdit::AdjustHeight(_, _) | MapEdit::ChangeHeight(_, _) => {
            commands.trigger(RemeshMap);
            change_player_pos = true;
            change_gold_pipe_pos = true;
            change_tiles_gizmos = true;
            for (_, mut shop_hop, mut viewport_obj) in shop_hop_boxes {
                let index = viewport_obj.editor.get_index_param();
                shop_hop.translation = get_shop_hop_pos(&file, file.file.shop_warp_tiles[index]);
                viewport_obj.old_pos = shop_hop.translation;
            }
        }
        MapEdit::ChangeConnection(_, _, _) | MapEdit::ChangeMaterial(_, _, _) => {
            commands.trigger(RemeshMap);
        }
        MapEdit::ChangePopupType(_, _)
        | MapEdit::ChangeCoins(_, _)
        | MapEdit::ChangeWalkOver(_, _)
        | MapEdit::ChangeSilverStarSpawnable(_, _) => {} // TODO: Make silver stars render on map
        MapEdit::AddAnimationGroup(_, _, _)
        | MapEdit::DeleteAnimationGroup(_)
        | MapEdit::RenameAnimationGroup(_, _, _, _)
        | MapEdit::ChangeAnimationGroupAnchor(_, _) => {
            commands.trigger(RemeshMap);
            change_anchor_gizmos = true;
        }
    }

    if change_player_pos {
        for (mut player, mut viewport_obj) in player {
            player.translation = get_player_pos(&file, file.file.starting_tile);
            viewport_obj.old_pos = player.translation;
        }
    }

    if change_gold_pipe_pos {
        for (mut gold_pipe, mut viewport_obj) in gold_pipe {
            gold_pipe.translation = get_gold_pipe_pos(&file, file.file.star_warp_tile);
            viewport_obj.old_pos = gold_pipe.translation;
        }
    }

    if change_podium_pos {
        for (mut podium, mut viewport_obj) in podium {
            podium.translation = get_podium_pos(&file, file.file.podium_position);
            viewport_obj.old_pos = podium.translation;
        }
    }

    if change_tiles_gizmos && let Ok((mut transform, mut object, gizmo)) = tiles_gizmo.single_mut()
    {
        let offset = get_tile_gizmo_mesh_offset(gizmo.0, &file);
        transform.translation = offset;
        object.old_pos = offset;
        tiles_gizmo_child.single_mut().unwrap().translation = -offset;
    }

    if change_anchor_gizmos {
        for (entity, mut transform, mut object) in anchor_gizmos.iter_mut() {
            if let EditObject::AnimationGroupAnchor(name) = &object.editor {
                if let Some(group) = file.file.animations.get(&name.to_string()) {
                    let pos = group.anchor.into();
                    transform.translation = pos;
                    object.old_pos = pos;
                } else {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn on_remesh_map(
    _: On<RemeshMap>,
    mut commands: Commands,
    old: Query<Entity, With<MapMeshMarker>>,
    file: Res<LoadedFile>,
    state: Res<ViewportState>,
    assets: Res<AssetServer>,
    mut highlighted: Query<(Entity, &TilesGizmoMesh)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let start = Instant::now();
    commands.spawn(mesh_map(
        &file.file,
        &file.animation_state,
        state.atlas_material.clone(),
        &assets,
        &mut materials,
        &mut meshes,
    ));
    if let Ok((highlighted, marker)) = highlighted.single_mut() {
        commands.entity(highlighted).insert(mesh_top_highlights(
            &file.file,
            marker.0,
            &mut materials,
            &mut meshes,
        ));
    }
    debug!("Meshed in {:?}", start.elapsed());

    for old in old {
        commands.entity(old).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn on_select_for_editing(
    on: On<SelectForEditing>,
    mut commands: Commands,
    mut gizmo_options: ResMut<GizmoOptions>,
    current_gizmos: Query<(Entity, &GizmoTarget)>,
    temporary_gizmos: Query<Entity, With<TemporaryViewportObject>>,
    player: Query<Entity, With<PlayerMarker>>,
    shop_hop_box: Query<(Entity, &ViewportObject), With<ShopHopBoxMarker>>,
    gold_pipe: Query<(Entity, &mut Visibility), (With<GoldPipeMarker>, Without<PodiumMarker>)>,
    podium: Query<(Entity, &mut Visibility), With<PodiumMarker>>,
    results_camera: Query<(Entity, &ViewportObject), With<ResultsCameraMarker>>,
    mut tiles_gizmo: Query<
        (&mut Transform, &mut TilesGizmo, &mut ViewportObject),
        (
            Without<TilesGizmoMesh>,
            Without<ShopHopBoxMarker>,
            Without<ResultsCameraMarker>,
        ),
    >,
    mut tiles_gizmo_children: Query<(&mut Transform, &mut TilesGizmoMesh)>,
    cameras: Query<(Entity, &CameraId)>,
    mut file: ResMut<LoadedFile>,
) {
    if on.exclusive {
        if current_gizmos
            .clone()
            .iter()
            .any(|(_, target)| target.is_active())
        {
            return;
        }
        for (gizmo, _) in current_gizmos {
            commands.entity(gizmo).remove::<GizmoTarget>();
        }
        for gizmo in temporary_gizmos {
            commands.entity(gizmo).despawn();
        }
        if file.selected_range.is_some() {
            file.selected_range = None;
        }
        *gizmo_options = EditObject::None.update_gizmos(*gizmo_options);
    }
    *gizmo_options = on.object.update_gizmos(*gizmo_options);

    match &on.object {
        EditObject::StartingTile => {
            for player in player {
                commands.entity(player).insert(GizmoTarget::default());
            }
        }
        EditObject::ShopWarpTile(index) => {
            for (shop_hop, object) in shop_hop_box {
                if object.editor.get_index_param() == *index {
                    commands.entity(shop_hop).insert(GizmoTarget::default());
                }
            }
        }
        EditObject::StarWarpTile => {
            for (gold_pipe, mut visible) in gold_pipe {
                *visible = Visibility::Visible;
                commands.entity(gold_pipe).insert(GizmoTarget::default());
            }
        }
        EditObject::PodiumPosition => {
            for (podium, mut visible) in podium {
                *visible = Visibility::Visible;
                commands.entity(podium).insert(GizmoTarget::default());
            }
        }
        EditObject::ResultsCamera(index) => {
            for (camera, object) in results_camera {
                if object.editor.get_index_param() == *index {
                    commands.entity(camera).insert(GizmoTarget::default());
                }
            }
        }
        EditObject::AnimationGroupAnchor(name) => {
            if let Some(group) = file.file.animations.get(&name.to_string()) {
                let spawn = group.anchor.into();
                commands.spawn((
                    ViewportObject {
                        editor: EditObject::AnimationGroupAnchor(name.clone()),
                        old_pos: spawn,
                        old_rot: None,
                    },
                    TemporaryViewportObject,
                    Transform::from_translation(spawn),
                    GizmoTarget::default(),
                    AnimationGroupAnchorGizmo,
                ));
            }
        }
        EditObject::MapSize(side) => {
            let spawn = get_bounds_gizmo_location(&file, *side);
            commands.spawn((
                ViewportObject {
                    editor: EditObject::MapSize(*side),
                    old_pos: spawn,
                    old_rot: None,
                },
                TemporaryViewportObject,
                BoundsGizmo(*side),
                Transform::from_translation(spawn),
                GizmoTarget::default(),
            ));
        }
        EditObject::Camera(target) => {
            for (camera, id) in cameras {
                if id == target {
                    commands.entity(camera).insert(GizmoTarget::default());
                }
            }
        }
        EditObject::Tile(new_pos) => {
            let new_pos = *new_pos;
            if !on.exclusive
                && let Ok((mut transform, mut tiles, mut object)) = tiles_gizmo.single_mut()
            {
                let old_start = tiles.0.start;
                match (new_pos.x >= old_start.x, new_pos.y >= old_start.y) {
                    (true, true) => {
                        tiles.0.end = new_pos;
                    }
                    (false, true) => {
                        tiles.0 = TileRange {
                            start: MpsVec2::new(new_pos.x, old_start.y),
                            end: MpsVec2::new(old_start.x, new_pos.y),
                        };
                    }
                    (true, false) => {
                        tiles.0 = TileRange {
                            start: MpsVec2::new(old_start.x, new_pos.y),
                            end: MpsVec2::new(new_pos.x, old_start.y),
                        };
                    }
                    (false, false) => {
                        tiles.0 = TileRange {
                            start: new_pos,
                            end: old_start,
                        };
                    }
                }
                file.selected_range = Some(tiles.0);

                let mesh_offset = get_tile_gizmo_mesh_offset(tiles.0, &file);
                transform.translation = mesh_offset;
                object.old_pos = mesh_offset;

                let mut child = tiles_gizmo_children.single_mut().unwrap();
                child.0.translation = -mesh_offset;
                child.1.0 = tiles.0;
            } else {
                let range = TileRange {
                    start: new_pos,
                    end: new_pos,
                };
                file.selected_range = Some(range);

                let mesh_offset = get_tile_gizmo_mesh_offset(range, &file);
                commands.spawn((
                    ViewportObject {
                        editor: EditObject::Tile(new_pos),
                        old_pos: mesh_offset,
                        old_rot: None,
                    },
                    TilesGizmo(range),
                    TemporaryViewportObject,
                    GizmoTarget::default(),
                    Transform::from_translation(mesh_offset),
                    Visibility::default(),
                    children![(
                        TilesGizmoMesh(range),
                        Transform::from_translation(-mesh_offset),
                        NoFrustumCulling,
                    )],
                ));
            }
            commands.trigger(RemeshMap);
        }
        EditObject::None => {}
    }
}

fn get_tile_gizmo_mesh_offset(range: TileRange, file: &LoadedFile) -> Vec3 {
    Vec3::new(
        (range.start.x + range.end.x) as f32 / 2.0,
        file.file.data[(
            (range.start.y + range.end.y) as usize / 2,
            (range.start.x + range.end.x) as usize / 2,
        )]
            .height
            .center_height() as f32,
        (range.start.y + range.end.y) as f32 / 2.0,
    )
}

fn get_player_pos(file: &LoadedFile, pos: MpsVec2) -> Vec3 {
    get_height_offset_pos(file, pos, 0.375)
}

fn get_shop_hop_pos(file: &LoadedFile, pos: MpsVec2) -> Vec3 {
    get_height_offset_pos(file, pos, 1.25)
}

fn get_gold_pipe_pos(file: &LoadedFile, pos: MpsVec2) -> Vec3 {
    get_height_offset_pos(file, pos, 1.375)
}

fn get_podium_pos(file: &LoadedFile, pos: MpsVec2) -> Vec3 {
    get_height_offset_pos(file, pos, 0.0)
}

fn get_height_offset_pos(file: &LoadedFile, pos: MpsVec2, offset: f32) -> Vec3 {
    let tile_y = file.file[pos].height.center_height();
    Vec3::new(pos.x as f32, tile_y as f32 + offset, pos.y as f32)
}

fn get_bounds_gizmo_location(file: &LoadedFile, side: Direction) -> Vec3 {
    let (rows, cols) = file.file.data.size();
    (match side {
        Direction::West => Vec3::new(0.0, 0.0, rows as f32 / 2.0),
        Direction::East => Vec3::new(cols as f32, 0.0, rows as f32 / 2.0),
        Direction::North => Vec3::new(cols as f32 / 2.0, 0.0, 0.0),
        Direction::South => Vec3::new(cols as f32 / 2.0, 0.0, rows as f32),
    }) - Vec3::new(0.5, 0.0, 0.5)
}

fn on_pointer_click(
    on: On<Pointer<Click>>,
    objects: Query<&ViewportObject>,
    current_gizmos: Query<&ViewportObject, With<GizmoTarget>>,
    meshes: Query<(), With<MapMeshMarker>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    if on.button != PointerButton::Primary {
        return;
    }
    let editor = if let Ok(object) = objects.get(on.entity) {
        if !object.editor.directly_usable() {
            return;
        }
        object.editor.clone()
    } else if meshes.contains(on.entity) {
        let coord_vec = on.hit.position.unwrap() - on.hit.normal.unwrap() * 0.001;
        let coord = MpsVec2::new(coord_vec.x.round() as i32, coord_vec.z.round() as i32);
        EditObject::Tile(coord)
    } else {
        return;
    };
    let exclusive = !keys.any_pressed(modifier_key!(Shift))
        || current_gizmos.iter().any(|x| !editor.same_type(&x.editor));
    commands.trigger(SelectForEditing {
        object: editor,
        exclusive,
    });
}

#[allow(clippy::too_many_arguments)]
fn on_preset_view(
    on: On<PresetView>,
    camera: Query<(Entity, &LookTransform, &Projection), With<Camera>>,
    selection: Query<Entity, With<GizmoTarget>>,
    mut commands: Commands,
    world: &World,
    player_pos: Query<&Transform, With<PlayerMarker>>,
    file: Res<LoadedFile>,
    meshes: Res<Assets<Mesh>>,
) {
    for (camera, transform, projection) in camera {
        let new_transform = match on.event() {
            PresetView::Player => {
                let Ok(player_pos) = player_pos.single_inner() else {
                    return;
                };
                get_player_cam_transform(player_pos.translation)
            }
            PresetView::Center => {
                let data = &file.file.data;
                let target = Vec3::new(
                    data.cols() as f32 / 2.0 - 0.5,
                    data[(data.rows() / 2, data.cols() / 2)]
                        .height
                        .center_height() as f32,
                    data.rows() as f32 / 2.0 - 0.5,
                );
                compute_grounded_look_transform(LookTransform {
                    eye: target + Vec3::new(-10.0, 10.0, 10.0),
                    target,
                    up: Vec3::Y,
                })
            }
            PresetView::Selection => {
                let Projection::Perspective(perspective) = projection else {
                    return;
                };
                let mut aabb: Option<Aabb3d> = None;
                for entity in selection {
                    let Some(new_aabb) = get_selected_entity_aabb(entity, world, &meshes) else {
                        continue;
                    };
                    if let Some(aabb) = aabb.as_mut() {
                        *aabb = aabb.merge(&new_aabb);
                    } else {
                        aabb = Some(new_aabb);
                    }
                }
                let Some(aabb) = aabb else {
                    return;
                };
                let aabb = Aabb::from_min_max(aabb.min.into(), aabb.max.into());

                let radius = aabb.half_extents.length();
                let aspect = perspective.aspect_ratio;
                let fov_y = perspective.fov;
                let fov_x = ((fov_y / 2.0).tan() * aspect).atan() * 2.0;
                let min_fov = fov_x.min(fov_y);
                let distance = radius / (min_fov / 2.0).sin();

                let mut current_unit = (transform.target - transform.eye).normalize();
                if current_unit.y > 0.0 {
                    current_unit.y = -current_unit.y;
                }
                compute_grounded_look_transform(LookTransform {
                    eye: Vec3::from(aabb.center) - current_unit * distance,
                    target: aabb.center.into(),
                    up: if current_unit.abs_diff_eq(Vec3::NEG_Y, 0.001) {
                        Vec3::NEG_Z
                    } else {
                        Vec3::Y
                    },
                })
            }
            PresetView::TopDown => {
                let Projection::Perspective(perspective) = projection else {
                    return;
                };
                let data = &file.file.data;
                let fov_tan = (perspective.fov / 2.0).tan();
                let w_distance =
                    (data.cols() as f32 / 2.0 + 0.5) / (fov_tan * perspective.aspect_ratio);
                let h_distance = (data.rows() as f32 / 2.0 + 0.5) / fov_tan;
                let base_height = data
                    .iter()
                    .map(|x| x.height.max_height() as f32)
                    .reduce(f32::max)
                    .unwrap_or_default();

                let target = Vec3::new(
                    data.cols() as f32 / 2.0 - 0.5,
                    0.0,
                    data.rows() as f32 / 2.0 - 0.5,
                );
                LookTransform {
                    eye: target.with_y(base_height + w_distance.max(h_distance).max(20.0)),
                    target,
                    up: Vec3::NEG_Z,
                }
            }
            PresetView::Transform(transform) => {
                let transform = Transform::from(*transform);
                let forwards = transform.forward().as_vec3();
                let new_transform = LookTransform {
                    eye: transform.translation,
                    target: transform.translation + forwards,
                    up: transform.up().as_vec3(),
                };
                if forwards.y < -0.001 {
                    compute_grounded_look_transform(new_transform)
                } else {
                    new_transform
                }
            }
        };
        commands.entity(camera).insert(transform.ease_to(
            new_transform,
            EaseFunction::QuinticInOut,
            EasingType::Once {
                duration: Duration::from_millis(300),
            },
        ));
    }
}

fn on_toggle_preview_visibility(
    on: On<TogglePreviewVisibility>,
    mut commands: Commands,
    query: Query<(
        Entity,
        &mut Visibility,
        &VisibilityToggleable,
        Option<&GizmoTarget>,
    )>,
) {
    for (entity, mut visibility, toggleable, gizmo_target) in query {
        if toggleable.0 == on.object {
            *visibility = if on.visible {
                Visibility::Visible
            } else {
                if let Some(target) = gizmo_target {
                    if target.is_active() {
                        continue;
                    }
                    commands.entity(entity).remove::<GizmoTarget>();
                }
                Visibility::Hidden
            };
        }
    }
}

fn on_preview_results_animation(
    _: On<PreviewResultsAnimation>,
    mut commands: Commands,
    camera: Query<Entity, With<Camera>>,
) {
    let Ok(camera) = camera.single_inner() else {
        return;
    };
    commands
        .entity(camera)
        .insert(ResultsAnimationPreview(Timer::from_seconds(
            10.0,
            TimerMode::Once,
        )));
}

fn get_selected_entity_aabb(
    entity: Entity,
    world: &World,
    meshes: &Assets<Mesh>,
) -> Option<Aabb3d> {
    let entity = world.entity(entity);
    if let Some(mesh) = entity.get::<Mesh3d>() {
        let aabb = meshes.get(&mesh.0)?.compute_aabb()?;
        let translation = entity
            .get::<Transform>()
            .map(|x| Vec3A::from(x.translation))
            .unwrap_or_default();
        Some(Aabb3d::new(aabb.center + translation, aabb.half_extents))
    } else if entity.contains::<TilesGizmo>() {
        let child = world.entity(entity.get::<Children>()?[0]);
        let mesh = child.get::<Mesh3d>()?;
        let aabb = meshes.get(&mesh.0)?.compute_aabb()?;
        Some(Aabb3d::new(aabb.center, aabb.half_extents))
    } else {
        entity
            .get::<Transform>()
            .map(|transform| Aabb3d::new(transform.translation, Vec3::splat(2.0)))
    }
}

fn get_player_cam_transform(player_pos: Vec3) -> LookTransform {
    compute_grounded_look_transform(LookTransform {
        eye: player_pos + Vec3::new(0.0, 3.0, 6.0),
        target: player_pos,
        up: Vec3::Y,
    })
}

fn compute_grounded_look_transform(transform: LookTransform) -> LookTransform {
    let look_angle = (transform.eye - transform.target).normalize();
    assert!(
        look_angle.y > 0.0,
        "compute_grounded_look_transform called while not looking down"
    );
    LookTransform {
        eye: transform.eye,
        target: Vec3::new(
            transform.eye.x - transform.eye.y / look_angle.y * look_angle.x,
            0.0,
            transform.eye.z - transform.eye.y / look_angle.y * look_angle.z,
        ),
        up: transform.up,
    }
}

fn keyboard_handler(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if shortcut_pressed!(keys, Alt + KeyA) {
        commands.trigger(SelectForEditing {
            object: EditObject::None,
            exclusive: true,
        })
    }
}

fn ensure_camera_up(camera: Query<(&mut LookTransform, &Transform), With<Camera>>) {
    for (mut look, real) in camera {
        if !real.up().abs_diff_eq(look.up, 0.001) && look.up != Vec3::Y {
            look.up = Vec3::Y;
        }
    }
}

fn update_gizmos(mut options: ResMut<GizmoOptions>, viewport: Res<ViewportTarget>) {
    options.viewport_rect = Some(Rect::from_corners(
        viewport.upper_left,
        viewport.upper_left + viewport.size,
    ));
}

fn sync_from_gizmos(
    mut commands: Commands,
    mut file: ResMut<LoadedFile>,
    gizmos: Query<
        (
            &mut Transform,
            &mut ViewportObject,
            &GizmoTarget,
            Option<&TilesGizmo>,
        ),
        Without<TilesGizmoMesh>,
    >,
    mut selected_mesh_gizmo: Query<&mut Transform, With<TilesGizmoMesh>>,
) {
    for (mut transform, mut object, gizmo, tiles) in gizmos {
        match object.editor.clone() {
            EditObject::StartingTile => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
                if gizmo.is_active() {
                    transform.translation = get_player_pos(&file, in_bounds_pos);
                } else {
                    file.edit_map(&mut commands, MapEdit::StartingTile(in_bounds_pos));
                    object.old_pos = pos;
                }
            }
            EditObject::AnimationGroupAnchor(name) => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                if !gizmo.is_active() {
                    file.edit_map(
                        &mut commands,
                        MapEdit::ChangeAnimationGroupAnchor(
                            name.to_string(),
                            MpsVec2f::new(pos.x as f64, pos.z as f64),
                        ),
                    );
                    object.old_pos = pos;
                }
            }
            EditObject::ShopWarpTile(index) => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
                if gizmo.is_active() {
                    transform.translation = get_shop_hop_pos(&file, in_bounds_pos);
                } else {
                    file.edit_map(
                        &mut commands,
                        MapEdit::ShopWarpTile(index, ListEdit::Set(in_bounds_pos)),
                    );
                    object.old_pos = pos;
                }
            }
            EditObject::StarWarpTile => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
                if gizmo.is_active() {
                    transform.translation = get_gold_pipe_pos(&file, in_bounds_pos);
                } else {
                    file.edit_map(&mut commands, MapEdit::StarWarpTile(in_bounds_pos));
                    object.old_pos = pos;
                }
            }
            EditObject::PodiumPosition => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
                if gizmo.is_active() {
                    transform.translation = get_podium_pos(&file, in_bounds_pos);
                } else {
                    file.edit_map(&mut commands, MapEdit::PodiumPosition(in_bounds_pos));
                    object.old_pos = pos;
                }
            }
            EditObject::ResultsCamera(index) => {
                if !gizmo.is_active() {
                    let pos = transform.translation;
                    let target_pos = get_podium_pos(&file, file.file.podium_position) + Vec3::Y;
                    if pos != object.old_pos {
                        file.edit_map(
                            &mut commands,
                            MapEdit::ResultsCamera(index, ListEdit::Set((pos - target_pos).into())),
                        );
                        object.old_pos = pos;
                    }
                }
            }
            EditObject::MapSize(side) => {
                let x_change = (transform.translation.x - object.old_pos.x).round() as i32;
                let y_change = (transform.translation.z - object.old_pos.z).round() as i32;
                if x_change == 0 && y_change == 0 {
                    continue;
                }
                let expand = MapEdit::ExpandMap(side, None);
                let shrink = MapEdit::ShrinkMap(side);
                match side {
                    Direction::West | Direction::North => {
                        let axis = if side == Direction::West {
                            x_change
                        } else {
                            y_change
                        };
                        if axis < 0 {
                            for _ in 0..axis.abs() {
                                file.edit_map(&mut commands, expand.clone());
                            }
                        } else if axis > 0 {
                            for _ in 0..axis {
                                file.edit_map(&mut commands, shrink.clone());
                            }
                        }
                        transform.translation = object.old_pos;
                    }
                    Direction::East | Direction::South => {
                        let axis = if side == Direction::East {
                            x_change
                        } else {
                            y_change
                        };
                        let mut changed = false;
                        if axis > 0 {
                            for _ in 0..axis {
                                changed |= file.edit_map(&mut commands, expand.clone());
                            }
                        } else if axis < 0 {
                            for _ in 0..axis.abs() {
                                changed |= file.edit_map(&mut commands, shrink.clone());
                            }
                        }
                        if changed {
                            object.old_pos = transform.translation;
                        } else {
                            transform.translation = object.old_pos;
                        }
                    }
                }

                let player_pos = file.in_bounds(file.file.starting_tile);
                file.edit_map(&mut commands, MapEdit::StartingTile(player_pos));

                for index in 0..file.file.shop_warp_tiles.len() {
                    let tile = file.in_bounds(file.file.shop_warp_tiles[index]);
                    file.edit_map(
                        &mut commands,
                        MapEdit::ShopWarpTile(index, ListEdit::Set(tile)),
                    );
                }

                let gold_pipe_pos = file.in_bounds(file.file.star_warp_tile);
                file.edit_map(&mut commands, MapEdit::StarWarpTile(gold_pipe_pos));

                let podium_pos = file.in_bounds(file.file.podium_position);
                file.edit_map(&mut commands, MapEdit::PodiumPosition(podium_pos));
            }
            EditObject::Camera(camera) => {
                if !gizmo.is_active() {
                    let pos = transform.translation;
                    let rot = transform.rotation;
                    if pos != object.old_pos {
                        file.edit_map(&mut commands, MapEdit::ChangeCameraPos(camera, pos.into()));
                        object.old_pos = pos;
                    }
                    if Some(rot) != object.old_rot {
                        file.edit_map(&mut commands, MapEdit::ChangeCameraRot(camera, rot.into()));
                        object.old_rot = Some(rot);
                    }
                }
            }
            EditObject::Tile(_) => {
                let range = tiles.unwrap().0;
                if gizmo.is_active() {
                    let Some(GizmoResult::Translation { delta, .. }) = gizmo.latest_result() else {
                        return;
                    };
                    let delta = (delta.y * 4.0) as i32 as f64 / 4.0;
                    file.file.adjust_height(range, delta);
                    selected_mesh_gizmo.single_mut().unwrap().translation.y -= delta as f32;
                    commands.trigger(RemeshMap);
                } else if transform.translation != object.old_pos {
                    let change = (transform.translation.y - object.old_pos.y) as f64;
                    let change = (change * 4.0) as i32 as f64 / 4.0;
                    file.file.adjust_height(range, -change);
                    selected_mesh_gizmo.single_mut().unwrap().translation.y += change as f32;
                    file.edit_map(&mut commands, MapEdit::AdjustHeight(range, change));
                    object.old_pos = transform.translation;
                }
            }
            EditObject::None => {}
        }
    }
}

fn update_lights(
    mut light: Query<&mut LookTransform, With<DirectionalLight>>,
    loaded_file: Res<LoadedFile>,
) {
    if let Ok(mut light) = light.single_mut() {
        let map_data = &loaded_file.file.data;
        light.eye = Vec3::new(
            map_data.cols() as f32 / 2.0,
            10.0,
            map_data.rows() as f32 / 2.0 + 5.0,
        );
        light.target = Vec3::new(
            map_data.cols() as f32 / 2.0,
            0.0,
            map_data.rows() as f32 / 2.0,
        );
    }
}

fn update_textures(
    mut textures: ResMut<ViewportState>,
    skybox: Query<&mut Skybox>,
    file: Res<LoadedFile>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if textures.skybox.outdated
        && let Some(fallback) = images.get(&textures.skybox.missing)
        && file.loaded_textures.skybox.iter().all(|x| {
            x.image == Handle::default()
                || matches!(
                    assets.load_state(&x.image),
                    LoadState::Loaded | LoadState::Failed(_)
                )
        })
    {
        assert_eq!(fallback.data_order, TextureDataOrder::MipMajor);
        assert_eq!(
            fallback.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        let fallback_data = fallback.data.as_ref().expect("Fallback skybox not on CPU");
        let fallback_stride = fallback.width() as usize * fallback.height() as usize * 4;

        let images = file.loaded_textures.skybox.each_ref().map(|x| {
            (x.image != Handle::default())
                .then(|| images.get(&x.image))
                .flatten()
        });
        let biggest = images
            .iter()
            .filter_map(|x| x.map(|x| x.width().max(x.height())))
            .max();
        if let Some(size) = biggest {
            let stride = size as usize * size as usize * 4;
            let mut result = Vec::with_capacity(stride * 6);
            for (i, image) in images.into_iter().enumerate() {
                let image = image
                    .and_then(|x| x.clone().try_into_dynamic().ok())
                    .unwrap_or_else(|| {
                        DynamicImage::ImageRgba8(
                            RgbaImage::from_vec(
                                fallback.width(),
                                fallback.height(),
                                fallback_data[i * fallback_stride..(i + 1) * fallback_stride]
                                    .to_vec(),
                            )
                            .unwrap(),
                        )
                    });
                result.extend(
                    if image.dimensions() == (size, size) {
                        image
                    } else {
                        image.resize_exact(size, size, FilterType::Triangle)
                    }
                    .into_rgba8()
                    .into_raw(),
                );
            }
            let mut image = Image::new(
                Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 6,
                },
                TextureDimension::D2,
                result,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.texture_view_descriptor = Some(TextureViewDescriptor {
                dimension: Some(TextureViewDimension::Cube),
                ..Default::default()
            });
            textures.skybox.current = assets.add(image);
        } else {
            textures.skybox.current = textures.skybox.missing.clone();
        }

        for mut skybox in skybox {
            skybox.image = textures.skybox.current.clone();
        }

        textures.skybox.outdated = false;
    }

    if textures.atlas.outdated {
        let image = &file.loaded_textures.atlas.image;
        textures.atlas.current = if *image == Handle::default() {
            textures.atlas.missing.clone()
        } else {
            image.clone()
        };
        materials
            .get_mut(&textures.atlas_material)
            .expect("atlas_material should exist")
            .base_color_texture = Some(textures.atlas.current.clone());
        textures.atlas.outdated = false;
    }
}

fn update_results_preview(
    mut commands: Commands,
    time: Res<Time>,
    mut controls: MessageReader<ControlMessage>,
    camera: Query<(Entity, &mut LookTransform, &mut ResultsAnimationPreview)>,
    file: Res<LoadedFile>,
) {
    if !controls.is_empty() {
        controls.clear();
        for (entity, _, _) in camera {
            commands.entity(entity).remove::<ResultsAnimationPreview>();
        }
        return;
    }

    let poses = &file.file.results_anim_cam_poses;
    let target_pos = get_podium_pos(&file, file.file.podium_position) + Vec3::Y;
    for (entity, mut transform, mut preview) in camera {
        preview.0.tick(time.delta());
        if preview.0.is_finished() {
            commands.entity(entity).remove::<ResultsAnimationPreview>();
            continue;
        }

        let base_index = preview.0.fraction() * (poses.len() - 2) as f32;
        let i = base_index as usize + 1;
        let t = (base_index % 1.0) as f64;

        let line1 = poses[i - 1].lerp(poses[i], 0.5).lerp(poses[i], t);
        let line2 = poses[i].lerp(poses[i + 1].lerp(poses[i], 0.5), t);
        let mut line = Vec3::from(line1.lerp(line2, t));

        if line == Vec3::ZERO {
            line = Vec3::Z * 0.001;
        }

        let new_transform = LookTransform {
            eye: target_pos + line,
            target: target_pos,
            up: Vec3::Y,
        };
        if line.y > 0.001 {
            *transform = compute_grounded_look_transform(new_transform);
        } else {
            *transform = new_transform;
        }
    }
}

fn update_results_cameras(
    mut cameras: Query<(&mut Transform, &ViewportObject), With<ResultsCameraMarker>>,
    mut gizmos: Gizmos,
    file: Res<LoadedFile>,
) {
    let cameras_poses = cameras
        .iter()
        .clone()
        .sort_by_key::<&ViewportObject, _>(|x| x.editor.get_index_param())
        .map(|(x, _)| x.translation)
        .collect_vec();
    let target = get_podium_pos(&file, file.file.podium_position) + Vec3::Y;
    for (mut transform, object) in &mut cameras {
        let index = object.editor.get_index_param();
        if index > 0 {
            gizmos.line(
                cameras_poses[index - 1],
                transform.translation,
                Srgba::rgb_u8(0, 200, 0),
            );
        }
        transform.look_at(target, Vec3::Y);
    }
}

fn custom_mouse_pick_events(
    mut window_events: MessageReader<WindowEvent>,
    viewport_target: Res<ViewportTarget>,
    mut cursor_last: Local<Vec2>,
    mut pointer_inputs: MessageWriter<PointerInput>,
) {
    if viewport_target.disable_input {
        return;
    }
    for window_event in window_events.read() {
        match window_event {
            WindowEvent::CursorMoved(event) => {
                let position = event.position - viewport_target.upper_left;
                if position.x < 0.0
                    || position.y < 0.0
                    || position.x > viewport_target.size.x
                    || position.y > viewport_target.size.y
                {
                    continue;
                }
                let location = Location {
                    target: NormalizedRenderTarget::Image(viewport_target.texture.clone().into()),
                    position,
                };
                pointer_inputs.write(PointerInput::new(
                    PointerId::Mouse,
                    location,
                    PointerAction::Move {
                        delta: position - *cursor_last,
                    },
                ));
                *cursor_last = position;
            }
            WindowEvent::MouseButtonInput(input) => {
                let location = Location {
                    target: NormalizedRenderTarget::Image(viewport_target.texture.clone().into()),
                    position: *cursor_last,
                };
                let button = match input.button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    MouseButton::Other(_) | MouseButton::Back | MouseButton::Forward => continue,
                };
                let action = match input.state {
                    ButtonState::Pressed => PointerAction::Press(button),
                    ButtonState::Released => PointerAction::Release(button),
                };
                pointer_inputs.write(PointerInput::new(PointerId::Mouse, location, action));
            }
            WindowEvent::MouseWheel(event) => {
                let MouseWheel {
                    unit,
                    x,
                    y,
                    window: _,
                } = *event;

                let location = Location {
                    target: NormalizedRenderTarget::Image(viewport_target.texture.clone().into()),
                    position: *cursor_last,
                };

                let action = PointerAction::Scroll { x, y, unit };

                pointer_inputs.write(PointerInput::new(PointerId::Mouse, location, action));
            }
            _ => {}
        }
    }
}
