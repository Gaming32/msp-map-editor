use crate::assets::{PlayerMarker, missing_atlas, missing_skybox, player};
use crate::load_file::{FileLoaded, LoadedFile};
use crate::mesh::{MapMeshMarker, mesh_map, mesh_top_highlights};
use crate::schema::MpsVec2;
use crate::sync::{Direction, EditObject, MapEdit, MapEdited, PresetView, SelectForEditing};
use crate::tile_range::TileRange;
use crate::{modifier_key, shortcut_pressed};
use bevy::asset::io::embedded::GetAssetServer;
use bevy::asset::{LoadState, RenderAssetUsages};
use bevy::camera::NormalizedRenderTarget;
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
use bevy_map_camera::controller::CameraControllerButtons;
use bevy_map_camera::{CameraControllerSettings, LookTransform, MapCamera, MapCameraPlugin};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::f32::consts::PI;
use std::time::{Duration, Instant};
use transform_gizmo_bevy::GizmoHotkeys;
use transform_gizmo_bevy::prelude::*;

#[derive(Resource)]
pub struct ViewportTarget {
    pub texture: Handle<Image>,
    pub upper_left: Vec2,
    pub size: Vec2,
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
        })
        .insert_resource(ViewportState {
            skybox: ViewportTextureSet::new(missing_skybox),
            atlas: ViewportTextureSet::new(missing_atlas),
            atlas_material,
        })
        .add_plugins((MapCameraPlugin, MeshPickingPlugin, TransformGizmoPlugin))
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
        .add_systems(
            Update,
            (
                keyboard_handler,
                ensure_camera_up,
                update_gizmos,
                sync_from_gizmos,
                update_lights,
                update_textures,
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
        snap_angle: PI / 4.0,
        snap_distance: 0.5,
        group_targets: false,
        snapping: true,
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
        Skybox {
            image: textures.skybox.current.clone(),
            brightness: 400.0, // Nits
            rotation: Quat::IDENTITY,
        },
        MapCamera,
        GizmoCamera,
        LookTransform::default(),
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
}

#[derive(Component)]
struct ViewportObject {
    editor: EditObject,
    old_pos: Vec3,
    old_rot: Option<Quat>,
}

#[derive(Component)]
struct TemporaryViewportObject;
#[derive(Component)]
struct BoundsGizmo(Direction);
#[derive(Component)]
struct TilesGizmo(TileRange);
#[derive(Component)]
struct TilesGizmoMesh(TileRange);

#[derive(Event)]
struct RemeshMap;

#[allow(clippy::too_many_arguments)]
fn on_file_load(
    _: On<FileLoaded>,
    mut commands: Commands,
    objects: Query<Entity, Or<(With<ViewportObject>, With<MapMeshMarker>)>>,
    mut camera: Query<(&mut LookTransform, &mut Skybox), With<Camera>>,
    mut state: ResMut<ViewportState>,
    assets: Res<AssetServer>,
    file: Res<LoadedFile>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for existing in objects.iter() {
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
            editor: EditObject::StartingPosition,
            old_pos: player_pos,
            old_rot: None,
        },
    ));
    for (mut camera, mut skybox) in camera.iter_mut() {
        *camera = get_player_cam_transform(player_pos);
        skybox.image = state.skybox.current.clone();
    }

    commands.trigger(RemeshMap);
}

#[allow(clippy::too_many_arguments)]
fn on_map_edited(
    on: On<MapEdited>,
    mut commands: Commands,
    file: Res<LoadedFile>,
    mut player: Query<
        (&mut Transform, &mut ViewportObject),
        (
            With<PlayerMarker>,
            Without<BoundsGizmo>,
            Without<TilesGizmo>,
            Without<TilesGizmoMesh>,
        ),
    >,
    mut bounds_markers: Query<
        (&mut Transform, &mut ViewportObject, &BoundsGizmo),
        (Without<TilesGizmo>, Without<TilesGizmoMesh>),
    >,
    mut tiles_gizmo: Query<
        (&mut Transform, &mut ViewportObject, &TilesGizmo),
        Without<TilesGizmoMesh>,
    >,
    mut tiles_gizmo_child: Query<&mut Transform, With<TilesGizmoMesh>>,
    mut textures: ResMut<ViewportState>,
) {
    let mut change_player_pos = false;
    let mut change_tiles_gizmos = false;

    match &on.0 {
        MapEdit::StartingPosition(_) => {
            change_player_pos = true;
        }
        MapEdit::Skybox(_, _) => {
            textures.skybox.outdated = true;
        }
        MapEdit::Atlas(_) => {
            textures.atlas.outdated = true;
        }
        MapEdit::ExpandMap(_, _) | MapEdit::ShrinkMap(_) => {
            commands.trigger(RemeshMap);
            change_player_pos = true;
            for (mut transform, mut object, bounds) in bounds_markers.iter_mut() {
                transform.translation = get_bounds_gizmo_location(&file, bounds.0);
                object.old_pos = transform.translation;
            }
        }
        MapEdit::AdjustHeight(_, _) | MapEdit::ChangeHeight(_, _) => {
            commands.trigger(RemeshMap);
            change_player_pos = true;
            change_tiles_gizmos = true;
        }
    }

    if change_player_pos {
        for (mut player, mut viewport_obj) in player.iter_mut() {
            player.translation = get_player_pos(&file, file.file.starting_tile);
            viewport_obj.old_pos = player.translation;
        }
    }

    if change_tiles_gizmos && let Ok((mut transform, mut object, gizmo)) = tiles_gizmo.single_mut()
    {
        let offset = get_tile_gizmo_mesh_offset(gizmo.0, &file);
        transform.translation = offset;
        object.old_pos = offset;
        tiles_gizmo_child.single_mut().unwrap().translation = -offset;
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
        &file.file.data,
        state.atlas_material.clone(),
        &assets,
        &mut materials,
        &mut meshes,
    ));
    if let Ok((highlighted, marker)) = highlighted.single_mut() {
        commands.entity(highlighted).insert(mesh_top_highlights(
            &file.file.data,
            marker.0,
            &mut materials,
            &mut meshes,
        ));
    }
    debug!("Meshed in {:?}", start.elapsed());

    for old in old.iter() {
        commands.entity(old).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
fn on_select_for_editing(
    on: On<SelectForEditing>,
    mut commands: Commands,
    mut gizmo_options: ResMut<GizmoOptions>,
    current_gizmos: Query<Entity, With<GizmoTarget>>,
    temporary_gizmos: Query<Entity, With<TemporaryViewportObject>>,
    player: Query<Entity, With<PlayerMarker>>,
    mut tiles_gizmo: Query<
        (&mut Transform, &mut TilesGizmo, &mut ViewportObject),
        Without<TilesGizmoMesh>,
    >,
    mut tiles_gizmo_children: Query<(&mut Transform, &mut TilesGizmoMesh)>,
    mut file: ResMut<LoadedFile>,
) {
    if on.exclusive {
        for gizmo in current_gizmos.iter() {
            commands.entity(gizmo).remove::<GizmoTarget>();
        }
        for gizmo in temporary_gizmos.iter() {
            commands.entity(gizmo).despawn();
        }
        if file.selected_range.is_some() {
            file.selected_range = None;
        }
    }

    match on.object {
        EditObject::StartingPosition => {
            *gizmo_options = GizmoOptions {
                gizmo_modes: GizmoMode::TranslateX | GizmoMode::TranslateZ | GizmoMode::TranslateXZ,
                snap_distance: 1.0,
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..Default::default()
                }),
                ..*gizmo_options
            };
            for player in player.iter() {
                commands.entity(player).insert(GizmoTarget::default());
            }
        }
        EditObject::MapSize(side) => {
            *gizmo_options = GizmoOptions {
                // One of these modes won't work, but since GizmoOptions are applied globally and not per-gizmo, this is all we can do.
                gizmo_modes: GizmoMode::TranslateX | GizmoMode::TranslateZ,
                snap_distance: 1.0,
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..Default::default()
                }),
                ..*gizmo_options
            };
            let spawn = get_bounds_gizmo_location(&file, side);
            commands.spawn((
                ViewportObject {
                    editor: EditObject::MapSize(side),
                    old_pos: spawn,
                    old_rot: None,
                },
                TemporaryViewportObject,
                BoundsGizmo(side),
                Transform::from_translation(spawn),
                GizmoTarget::default(),
            ));
        }
        EditObject::Tile(new_pos) => {
            *gizmo_options = GizmoOptions {
                gizmo_modes: GizmoMode::TranslateY.into(),
                snap_distance: 0.5,
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    ..Default::default()
                }),
                ..*gizmo_options
            };
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
                        Transform::from_translation(-mesh_offset)
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
    let tile_y = file
        .file
        .data
        .get(pos.y, pos.x)
        .map_or(0.0, |tile| tile.height.center_height());
    Vec3::new(pos.x as f32, tile_y as f32 + 0.375, pos.y as f32)
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
        object.editor
    } else if meshes.contains(on.entity) {
        let coord_vec = on.hit.position.unwrap() - on.hit.normal.unwrap() * 0.001;
        let coord = MpsVec2::new(coord_vec.x.round() as i32, coord_vec.z.round() as i32);
        EditObject::Tile(coord)
    } else {
        return;
    };
    commands.trigger(SelectForEditing {
        object: editor,
        exclusive: editor.exclusive_only() || !keys.any_pressed(modifier_key!(Shift)),
    });
}

fn on_preset_view(
    on: On<PresetView>,
    mut camera: Query<(Entity, &LookTransform, &Projection), With<Camera>>,
    mut commands: Commands,
    player_pos: Query<&Transform, With<PlayerMarker>>,
    file: Res<LoadedFile>,
) {
    for (camera, transform, projection) in camera.iter_mut() {
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
                    0.0,
                    data.rows() as f32 / 2.0 - 0.5,
                );
                LookTransform {
                    eye: target + Vec3::new(-10.0, 10.0, 10.0),
                    target,
                    up: Vec3::Y,
                }
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

fn get_player_cam_transform(player_pos: Vec3) -> LookTransform {
    const CAM_OFFSET: Vec3 = Vec3::new(0.0, 3.0, 6.0);
    let eye = player_pos + CAM_OFFSET;
    LookTransform {
        eye,
        target: Vec3::new(
            player_pos.x,
            0.0,
            eye.z - eye.y / CAM_OFFSET.y * CAM_OFFSET.z,
        ),
        up: Vec3::Y,
    }
}

fn keyboard_handler(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    file: Res<LoadedFile>,
) {
    if shortcut_pressed!(keys, Alt + KeyA) {
        commands.trigger(SelectForEditing {
            object: EditObject::None,
            exclusive: true,
        })
    } else if shortcut_pressed!(keys, KeyA) {
        let data = &file.file.data;
        commands.trigger(SelectForEditing {
            object: EditObject::Tile(MpsVec2::new(0, 0)),
            exclusive: true,
        });
        commands.trigger(SelectForEditing {
            object: EditObject::Tile(MpsVec2::new(data.cols() as i32 - 1, data.rows() as i32 - 1)),
            exclusive: false,
        });
    }
}

fn ensure_camera_up(mut camera: Query<(&mut LookTransform, &Transform), With<Camera>>) {
    for (mut look, real) in camera.iter_mut() {
        if !real.forward().abs_diff_eq(Vec3::NEG_Y, 0.001) && look.up != Vec3::Y {
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
    mut gizmos: Query<
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
    for (mut transform, mut object, gizmo, tiles) in gizmos.iter_mut() {
        match object.editor {
            EditObject::StartingPosition => {
                let pos = transform.translation;
                if pos == object.old_pos {
                    continue;
                }
                let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
                if gizmo.is_active() {
                    transform.translation = get_player_pos(&file, in_bounds_pos);
                } else {
                    file.edit_map(&mut commands, MapEdit::StartingPosition(in_bounds_pos));
                    object.old_pos = pos;
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
    mut skybox: Query<&mut Skybox>,
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

        for mut skybox in skybox.iter_mut() {
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

fn custom_mouse_pick_events(
    mut window_events: MessageReader<WindowEvent>,
    viewport_target: Res<ViewportTarget>,
    mut cursor_last: Local<Vec2>,
    mut pointer_inputs: MessageWriter<PointerInput>,
) {
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
