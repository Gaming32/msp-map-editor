use crate::load_file::{LoadedFile, new_file, open_file, save_file, save_file_as};
use crate::schema::MapFile;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowCloseRequested};
use bevy_file_dialog::prelude::*;
use bevy_mod_imgui::prelude::*;
use std::mem;

pub struct MapEditorUi;

impl Plugin for MapEditorUi {
    fn build(&self, app: &mut App) {
        app.insert_resource(UiState {
            // close_test_future: None,
            pending_close_state: PendingCloseState::None,
        })
        .add_plugins((
            ImguiPlugin::default(),
            FileDialogPlugin::new()
                .with_load_file::<MapFile>()
                .with_save_file::<MapFile>(),
        ))
        .add_systems(Startup, |mut imgui: NonSendMut<ImguiContext>| {
            imgui.with_io_mut(|io| {
                io.config_docking_always_tab_bar = true;
            });
        })
        .add_systems(Update, (imgui, keyboard_handler, close_handler));
    }
}

#[derive(Resource)]
pub struct UiState {
    // close_test_future: Option<Task<bool>>,
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

fn imgui(
    mut context: NonSendMut<ImguiContext>,
    mut state: ResMut<UiState>,
    current_open_file: Res<LoadedFile>,
    mut commands: Commands,
    window_query: Query<Entity, With<PrimaryWindow>>,
) {
    let ui = context.ui();

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
    // primary_window: Query<Entity, With<PrimaryWindow>>,
    mut open_file: ResMut<LoadedFile>,
) {
    for event in close_requested.read() {
        let window = event.window;
        ui_state.request_close_file(move |commands, _| {
            commands.entity(window).despawn();
        });

        // let window = event.window;
        // if open_file.dirty {
        //     ui_state.close_test_future = Some(AsyncComputeTaskPool::get().spawn(async move {
        //         native_dialog::MessageDialogBuilder::default()
        //             .set_title(TITLE)
        //             .set_text("You have unsaved changes. Are you sure you'd like to exit?")
        //             .set_level(MessageLevel::Warning)
        //             .confirm()
        //             .spawn()
        //             .await
        //             .unwrap_or_default()
        //     }));
        // } else {
        //     commands.entity(window).despawn();
        // }
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

    // if let Some(closing) = &mut ui_state.close_test_future
    //     && let Some(close) = check_ready(closing)
    // {
    //     if close && let Ok(window) = primary_window.single_inner() {
    //         commands.entity(window).despawn();
    //     }
    //     ui_state.close_test_future = None;
    // }
}
