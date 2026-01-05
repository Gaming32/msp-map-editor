use crate::assets::{PlayerMarker, missing_atlas, missing_skybox, player};
use crate::load_file::{FileLoaded, LoadedFile};
use crate::mesh::{MapMeshMarker, mesh_map};
use crate::schema::MpsVec2;
use crate::sync::{EditObject, MapSettingChanged, SelectForEditing};
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
use std::f32::consts::{FRAC_PI_2, PI};
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
        .add_observer(on_map_setting_changed)
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
        SpotLight {
            range: 500.0,
            radius: 500.0,
            intensity: 250_000.0,
            outer_angle: FRAC_PI_2,
            inner_angle: FRAC_PI_2,
            ..Default::default()
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
}

#[derive(Component)]
struct ViewportObject {
    editor: EditObject,
    old_pos: Vec3,
    old_rot: Option<Quat>,
}

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
        camera.eye = player_pos + Vec3::new(0.0, 4.0, 8.0);
        camera.target = player_pos;
        skybox.image = state.skybox.current.clone();
    }

    let start = Instant::now();
    commands.spawn(mesh_map(
        &file.file.data,
        state.atlas_material.clone(),
        &assets,
    ));
    info!("Took {:?} to mesh", start.elapsed());
}

fn on_map_setting_changed(
    on: On<MapSettingChanged>,
    file: Res<LoadedFile>,
    mut player: Query<(&mut Transform, &mut ViewportObject), With<PlayerMarker>>,
    mut textures: ResMut<ViewportState>,
) {
    match on.event() {
        MapSettingChanged::StartingPosition(pos) => {
            for (mut player, mut viewport_obj) in player.iter_mut() {
                player.translation = get_player_pos(&file, *pos);
                viewport_obj.old_pos = player.translation;
            }
        }
        MapSettingChanged::Skybox(_, _) => {
            textures.skybox.outdated = true;
        }
    }
}

fn on_select_for_editing(
    on: On<SelectForEditing>,
    mut commands: Commands,
    mut gizmo_options: ResMut<GizmoOptions>,
    current_gizmos: Query<Entity, With<GizmoTarget>>,
    player: Query<Entity, With<PlayerMarker>>,
) {
    if on.exclusive {
        for gizmo in current_gizmos.iter() {
            commands.entity(gizmo).remove::<GizmoTarget>();
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
    commands.trigger(SelectForEditing {
        object: if let Ok(object) = objects.get(on.entity) {
            object.editor
        } else {
            // TODO: Support selecting mesh
            return;
        },
        exclusive: !keys.any_pressed(modifier_key!(Shift)),
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
    mut player: Query<(&Transform, &mut ViewportObject), (With<PlayerMarker>, With<GizmoTarget>)>,
) {
    for (player_transform, mut old_transform) in player.iter_mut() {
        let pos = player_transform.translation;
        if pos != old_transform.old_pos {
            let in_bounds_pos = file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32));
            file.change_map_setting(
                &mut commands,
                MapSettingChanged::StartingPosition(in_bounds_pos),
            );
            old_transform.old_pos = pos;
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
        textures.skybox.outdated = false;

        for mut skybox in skybox.iter_mut() {
            skybox.image = textures.skybox.current.clone();
        }
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
