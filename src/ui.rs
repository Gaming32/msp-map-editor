use crate::Directories;
use crate::docking::UiDocking;
use crate::load_file::{LoadedFile, new_file, open_file, save_file, save_file_as};
use crate::schema::MapFile;
use crate::viewport::ViewportTarget;
use bevy::prelude::Image as BevyImage;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use bevy_file_dialog::prelude::*;
use bevy_mod_imgui::prelude::*;
use imgui::Image as ImguiImage;
use std::mem;
use std::time::Duration;

pub struct MapEditorUi;

impl Plugin for MapEditorUi {
    fn build(&self, app: &mut App) {
        app.world().resource::<Directories>();

        let mut imgui_plugin = ImguiPlugin::default();
        if let Some(dirs) = app.world().get_resource::<Directories>() {
            imgui_plugin.ini_filename = Some(dirs.data.join("imgui.ini"));
        }

        app.insert_resource(UiState {
            free_timer: Timer::new(Duration::from_millis(500), TimerMode::Repeating),
            ..Default::default()
        })
        .add_plugins((
            imgui_plugin,
            FileDialogPlugin::new()
                .with_load_file::<MapFile>()
                .with_save_file::<MapFile>(),
        ))
        .add_systems(Startup, |mut imgui: NonSendMut<ImguiContext>| {
            imgui.with_io_mut(|io| {
                io.config_docking_always_tab_bar = true;
            });
        })
        .add_systems(Update, (draw_imgui, keyboard_handler, close_handler));
    }
}

#[derive(Resource, Default)]
pub struct UiState {
    setup_complete: bool,
    viewport_texture: Option<TextureId>,
    textures_to_free: Vec<TextureId>,
    free_timer: Timer,
    pending_close_state: PendingCloseState,
}

type BoxedCloseHandler = Box<dyn FnOnce(&mut Commands, &mut LoadedFile) + Send + Sync>;

#[derive(Default)]
enum PendingCloseState {
    #[default]
    None,
    PendingUi(BoxedCloseHandler),
    PendingUserInput(BoxedCloseHandler),
    Confirmed(BoxedCloseHandler),
}

impl UiState {
    pub fn request_close_file(
        &mut self,
        action: impl FnOnce(&mut Commands, &mut LoadedFile) + Send + Sync + 'static,
    ) {
        self.pending_close_state = PendingCloseState::PendingUi(Box::new(action));
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "This system requires a lot of arguments"
)]
fn draw_imgui(
    mut context: NonSendMut<ImguiContext>,
    mut state: ResMut<UiState>,
    time: Res<Time>,
    current_open_file: Res<LoadedFile>,
    mut commands: Commands,
    window_query: Query<Entity, With<PrimaryWindow>>,
    mut viewport_target: ResMut<ViewportTarget>,
    mut images: ResMut<Assets<BevyImage>>,
) {
    if state.viewport_texture.is_none() {
        state.viewport_texture =
            Some(context.register_bevy_texture(viewport_target.texture.clone()));
    }

    state.free_timer.tick(time.delta());
    if state.free_timer.just_finished() && !state.textures_to_free.is_empty() {
        let len = state.textures_to_free.len() - 1;
        for texture in state.textures_to_free.drain(..len) {
            context.unregister_bevy_texture(&texture);
        }
    }

    let ui = context.ui();

    if !state.setup_complete {
        ui.dockspace_over_viewport().split(
            Direction::Left,
            0.8,
            |left| {
                left.dock_window("Viewport");
            },
            |right| {
                right.dock_window("Map settings");
                right.dock_window("Tile settings");
            },
        );
        state.setup_complete = true;
    } else {
        ui.dockspace_over_main_viewport();
    }

    ui.main_menu_bar(|| {
        ui.menu("File", || {
            if ui.menu_item_config("New").shortcut("Ctrl+N").build() {
                new_file(&mut state);
            }

            if ui.menu_item_config("Open").shortcut("Ctrl+O").build() {
                open_file(&mut state);
            }

            if ui.menu_item_config("Save").shortcut("Ctrl+S").build() {
                save_file(&mut commands, &current_open_file);
            }

            if ui
                .menu_item_config("Save as")
                .shortcut("Ctrl+Shift+S")
                .build()
            {
                save_file_as(&mut commands, &current_open_file);
            }

            ui.separator();

            if ui.menu_item_config("Quit").shortcut("Alt+F4").build()
                && let Ok(window) = window_query.single_inner()
            {
                commands.write_message(WindowCloseRequested { window });
            }
        });
    });

    ui.window("Viewport").collapsible(true).build(|| {
        if let Some(texture) = state.viewport_texture {
            let dest_size = ui.content_region_avail();
            if dest_size[0] < 1.0 || dest_size[1] < 1.0 {
                return;
            }
            let target_size = UVec2::new(dest_size[0] as u32, dest_size[1] as u32);
            if images
                .get(&viewport_target.texture)
                .is_some_and(|i| i.size() != target_size)
            {
                let real_image = images.get_mut(&viewport_target.texture).unwrap();
                real_image.resize_in_place(Extent3d {
                    width: target_size.x,
                    height: target_size.y,
                    depth_or_array_layers: 1,
                });
                state.textures_to_free.push(texture);
                state.viewport_texture = None;
            }
            viewport_target.upper_left = ui.cursor_screen_pos().into();
            viewport_target.size = dest_size.into();
            ImguiImage::new(texture, dest_size).build(ui);
        }
    });

    ui.window("Map settings").collapsible(true).build(|| {
        ui.text("MAP EDITOR");
    });

    ui.window("Tile settings").collapsible(true).build(|| {
        ui.text("No tile selected");
    });

    match mem::take(&mut state.pending_close_state) {
        PendingCloseState::PendingUi(action) if current_open_file.dirty => {
            ui.open_popup("Are you sure?");
            state.pending_close_state = PendingCloseState::PendingUserInput(action);
        }
        other => {
            state.pending_close_state = other;
        }
    }
    ui.modal_popup("Are you sure?", || {
        ui.text("The current file has not been saved. Are you sure?");

        let mut confirm = false;
        let mut close = false;
        if ui.button("Cancel") {
            state.pending_close_state = PendingCloseState::None;
            close = true;
        }
        ui.same_line();
        if ui.button("Don't Save") {
            confirm = true;
            close = true;
        }
        ui.same_line();
        if ui.button("Save") {
            save_file(&mut commands, &current_open_file);
            close = true;
        }

        if confirm
            && let PendingCloseState::PendingUserInput(action) =
                mem::take(&mut state.pending_close_state)
        {
            state.pending_close_state = PendingCloseState::Confirmed(action);
        }
        if close {
            ui.close_current_popup();
        }
    });
}

fn keyboard_handler(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut ui_state: ResMut<UiState>,
    current_open_file: Res<LoadedFile>,
) {
    macro_rules! modifier_key {
        (Ctrl) => {
            [KeyCode::ControlLeft, KeyCode::ControlRight]
        };
        (Shift) => {
            [KeyCode::ShiftLeft, KeyCode::ShiftRight]
        };
        (Alt) => {
            [KeyCode::AltLeft, KeyCode::AltRight]
        };
    }

    macro_rules! shortcut_pressed {
        ($key:ident) => {
            keys.just_pressed(KeyCode::$key)
        };
        ($modifier:ident + $($shortcut:tt)+) => {
            shortcut_pressed!($($shortcut)+)
                && keys.any_pressed(modifier_key!($modifier))
        };
    }

    if shortcut_pressed!(Ctrl + KeyN) {
        new_file(&mut ui_state);
    }
    if shortcut_pressed!(Ctrl + KeyO) {
        open_file(&mut ui_state);
    }
    if shortcut_pressed!(Ctrl + KeyS) {
        save_file(&mut commands, &current_open_file);
    }
    if shortcut_pressed!(Ctrl + Shift + KeyS) {
        save_file_as(&mut commands, &current_open_file);
    }
}

fn close_handler(
    mut commands: Commands,
    mut close_requested: MessageReader<WindowCloseRequested>,
    mut ui_state: ResMut<UiState>,
    mut open_file: ResMut<LoadedFile>,
) {
    for event in close_requested.read() {
        let window = event.window;
        ui_state.request_close_file(move |commands, _| {
            commands.entity(window).despawn();
        });
    }

    match mem::take(&mut ui_state.pending_close_state) {
        PendingCloseState::Confirmed(action) => {
            action(&mut commands, &mut open_file);
        }
        PendingCloseState::PendingUi(action) | PendingCloseState::PendingUserInput(action)
            if !open_file.dirty =>
        {
            action(&mut commands, &mut open_file);
        }
        other => {
            ui_state.pending_close_state = other;
        }
    }
}
