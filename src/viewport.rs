use crate::assets::{PlayerMarker, player};
use crate::load_file::{FileLoaded, LoadedFile};
use crate::schema::MpsVec2;
use crate::shortcut_pressed;
use crate::sync::{MapSettingChanged, SelectForEditing};
use bevy::camera::NormalizedRenderTarget;
use bevy::ecs::query::QueryFilter;
use bevy::input::ButtonState;
use bevy::input::mouse::MouseWheel;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::Rect;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::window::WindowEvent;
use bevy_map_camera::controller::CameraControllerButtons;
use bevy_map_camera::{CameraControllerSettings, LookTransform, MapCamera, MapCameraPlugin};
use std::f32::consts::PI;
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
        app.insert_resource(ViewportTarget {
            texture: render_texture,
            upper_left: Vec2::default(),
            size: Vec2::new(1.0, 1.0),
        })
        .add_plugins((MapCameraPlugin, TransformGizmoPlugin))
        .add_systems(
            First,
            custom_mouse_pick_events.in_set(PickingSystems::Input),
        )
        .add_systems(Startup, setup_viewport)
        .add_observer(on_file_load)
        .add_observer(on_map_setting_changed)
        .add_observer(on_select_for_editing)
        .add_systems(
            Update,
            (
                keyboard_handler,
                update_gizmos,
                sync_from_gizmos,
                update_lights,
            ),
        );
    }
}

fn setup_viewport(mut commands: Commands, viewport_target: Res<ViewportTarget>) {
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
        MapCamera,
        GizmoCamera,
        LookTransform::default(),
    ));

    commands.spawn((
        LookTransform::default(),
        DirectionalLight {
            shadows_enabled: true,
            ..Default::default()
        },
    ));
}

#[derive(Component)]
struct ViewportObject {
    old_pos: Vec3,
    old_rot: Option<Quat>,
}

fn on_file_load(
    _: On<FileLoaded>,
    mut commands: Commands,
    objects: Query<Entity, With<ViewportObject>>,
    mut camera: Query<&mut LookTransform, With<Camera>>,
    assets: Res<AssetServer>,
    file: Res<LoadedFile>,
) {
    for existing in objects.iter() {
        commands.entity(existing).despawn();
    }

    let player_pos = get_player_pos(&file, file.file.starting_tile);
    commands.spawn((
        player(&assets, player_pos),
        ViewportObject {
            old_pos: player_pos,
            old_rot: None,
        },
    ));
    for mut camera in camera.iter_mut() {
        camera.eye = player_pos + Vec3::new(0.0, 4.0, 8.0);
        camera.target = player_pos;
    }
}

fn on_map_setting_changed(
    on: On<MapSettingChanged>,
    file: Res<LoadedFile>,
    mut player: Query<(&mut Transform, &mut ViewportObject), With<PlayerMarker>>,
) {
    match on.event() {
        MapSettingChanged::StartingPosition(pos) => {
            for (mut player, mut viewport_obj) in player.iter_mut() {
                player.translation = get_player_pos(&file, *pos);
                viewport_obj.old_pos = player.translation;
            }
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
    match on.event() {
        SelectForEditing::StartingPosition => {
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
            clear_all_gizmos(&mut commands, &current_gizmos);
            for player in player.iter() {
                commands.entity(player).insert(GizmoTarget::default());
            }
        }
    }
}

fn get_player_pos(file: &LoadedFile, pos: MpsVec2) -> Vec3 {
    let tile_y = file
        .file
        .data
        .get(pos.x, pos.y)
        .map_or(0.0, |tile| tile.height.center_height());
    Vec3::new(pos.x as f32, tile_y as f32 + 0.375, pos.y as f32)
}

fn keyboard_handler(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    current_gizmos: Query<Entity, With<GizmoTarget>>,
) {
    if shortcut_pressed!(keys, Alt + KeyA) {
        clear_all_gizmos(&mut commands, &current_gizmos);
    }
}

fn clear_all_gizmos<F: QueryFilter>(commands: &mut Commands, current_gizmos: &Query<Entity, F>) {
    for gizmo in current_gizmos.iter() {
        commands.entity(gizmo).remove::<GizmoTarget>();
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
    file: Res<LoadedFile>,
    mut player: Query<(&Transform, &mut ViewportObject), (With<PlayerMarker>, With<GizmoTarget>)>,
) {
    for (player_transform, mut old_transform) in player.iter_mut() {
        let pos = player_transform.translation;
        if pos != old_transform.old_pos {
            commands.trigger(MapSettingChanged::StartingPosition(
                file.in_bounds(MpsVec2::new(pos.x as i32, pos.z as i32)),
            ));
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
