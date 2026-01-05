use crate::load_file::LoadedTexture;
use crate::schema::{MpsVec2, MpsVec3};
use bevy::prelude::Event;

#[derive(Event, Clone, Debug)]
pub struct MapEdited(pub MapEdit);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapEdit {
    StartingPosition(MpsVec2),
    Skybox(usize, LoadedTexture),
    Atlas(LoadedTexture),
}

#[derive(Event, Copy, Clone, Debug)]
pub struct SelectForEditing {
    pub object: EditObject,
    pub exclusive: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum EditObject {
    StartingPosition,
    None,
}

impl From<MpsVec2> for mint::Vector2<i32> {
    fn from(value: MpsVec2) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<mint::Vector2<i32>> for MpsVec2 {
    fn from(value: mint::Vector2<i32>) -> Self {
        Self {
            x: value.x,
            y: value.y,
        }
    }
}

impl From<MpsVec3> for mint::Vector3<f64> {
    fn from(value: MpsVec3) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<mint::Vector3<f64>> for MpsVec3 {
    fn from(value: mint::Vector3<f64>) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}
