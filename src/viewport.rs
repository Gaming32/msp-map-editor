use crate::assets::{PlayerMarker, missing_atlas, missing_skybox, player};
use crate::load_file::{FileLoaded, LoadedFile};
use crate::mesh::{MapMeshMarker, mesh_map};
use crate::schema::MpsVec2;
use crate::sync::{Direction, EditObject, MapEdit, MapEdited, SelectForEditing};
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
use bevy_map_camera::controller::CameraControllerButtons;
use bevy_map_camera::{CameraControllerSettings, LookTransform, MapCamera, MapCameraPlugin};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::f32::consts::PI;
use std::time::Instant;
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
        .add_systems(
            Update,
            (
                keyboard_handler,
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
        snap_distance: 1.0,
        group_targets: false,
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
struct BoundsGizmoMarker(Direction);

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
        const CAM_OFFSET: Vec3 = Vec3::new(0.0, 3.0, 6.0);
        camera.eye = player_pos + CAM_OFFSET;
        camera.target = Vec3::new(
            player_pos.x,
            0.0,
            camera.eye.z - camera.eye.y / CAM_OFFSET.y * CAM_OFFSET.z,
        );
        skybox.image = state.skybox.current.clone();
    }

    commands.trigger(RemeshMap);
}

fn on_map_edited(
    on: On<MapEdited>,
    mut commands: Commands,
    file: Res<LoadedFile>,
    mut player: Query<
        (&mut Transform, &mut ViewportObject),
        (With<PlayerMarker>, Without<BoundsGizmoMarker>),
    >,
    mut bounds_markers: Query<(&mut Transform, &mut ViewportObject, &BoundsGizmoMarker)>,
    mut textures: ResMut<ViewportState>,
) {
    let mut change_player_pos = false;
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
    }

    if change_player_pos {
        for (mut player, mut viewport_obj) in player.iter_mut() {
            player.translation = get_player_pos(&file, file.file.starting_tile);
            viewport_obj.old_pos = player.translation;
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
    info!("Meshed in {:?}", start.elapsed());

    for old in old.iter() {
        commands.entity(old).despawn();
    }
}

fn on_select_for_editing(
    on: On<SelectForEditing>,
    mut commands: Commands,
    mut gizmo_options: ResMut<GizmoOptions>,
    current_gizmos: Query<Entity, With<GizmoTarget>>,
    temporary_gizmos: Query<Entity, With<TemporaryViewportObject>>,
    player: Query<Entity, With<PlayerMarker>>,
    file: Res<LoadedFile>,
) {
    if on.exclusive {
        for gizmo in current_gizmos.iter() {
            commands.entity(gizmo).remove::<GizmoTarget>();
        }
        for gizmo in temporary_gizmos.iter() {
            commands.entity(gizmo).despawn();
        }
    }

    match on.object {
        EditObject::StartingPosition => {
            *gizmo_options = GizmoOptions {
                gizmo_modes: GizmoMode::TranslateX | GizmoMode::TranslateZ | GizmoMode::TranslateXZ,
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..Default::default()
                }),
                snapping: true,
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
                hotkeys: Some(GizmoHotkeys {
                    enable_snapping: None,
                    enable_accurate_mode: None,
                    ..Default::default()
                }),
                snapping: true,
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
                BoundsGizmoMarker(side),
                Transform::from_translation(spawn),
                GizmoTarget::default(),
            ));
        }
        EditObject::None => {}
    }
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
    _meshes: Query<(), With<MapMeshMarker>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
) {
    if on.button != PointerButton::Primary {
        return;
    }
    let editor = if let Ok(object) = objects.get(on.entity) {
        object.editor
    } else {
        // TODO: Support selecting mesh
        return;
    };
    if !editor.directly_usable() {
        return;
    }
    commands.trigger(SelectForEditing {
        object: editor,
        exclusive: editor.exclusive_only() || !keys.any_pressed(modifier_key!(Shift)),
    });
}

fn keyboard_handler(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if shortcut_pressed!(keys, Alt + KeyA) {
        commands.trigger(SelectForEditing {
            object: EditObject::None,
            exclusive: true,
        })
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
    mut gizmos: Query<(&mut Transform, &mut ViewportObject, &GizmoTarget)>,
) {
    for (mut transform, mut object, gizmo) in gizmos.iter_mut() {
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
