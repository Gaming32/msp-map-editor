#[macro_export]
macro_rules! modifier_key {
    (Ctrl) => {
        [
            ::bevy::prelude::KeyCode::ControlLeft,
            ::bevy::prelude::KeyCode::ControlRight,
        ]
    };
    (Shift) => {
        [
            ::bevy::prelude::KeyCode::ShiftLeft,
            ::bevy::prelude::KeyCode::ShiftRight,
        ]
    };
    (Alt) => {
        [
            ::bevy::prelude::KeyCode::AltLeft,
            ::bevy::prelude::KeyCode::AltRight,
        ]
    };
}

#[macro_export]
macro_rules! shortcut_pressed {
    ($keys:expr, $key:ident) => {
        $keys.just_pressed(::bevy::prelude::KeyCode::$key)
    };
    ($keys:expr, $modifier:ident + $($shortcut:tt)+) => {
        $crate::shortcut_pressed!($keys, $($shortcut)+)
            && $keys.any_pressed($crate::modifier_key!($modifier))
    };
}
