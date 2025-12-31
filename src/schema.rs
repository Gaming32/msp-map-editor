use monostate::{MustBe, MustBeBool};
use serde::{Deserialize, Serialize};
use serde_with::OneOrMany;
use serde_with::serde_as;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapFile {
    pub starting_tile: MpsVec2,
    pub shop_warp_tiles: Vec<MpsVec2>,
    pub tutorial_star: MpsTransform,
    pub tutorial_shop: MpsTransform,
    pub skybox: CubeMap,
    pub dark_skybox: CubeMap,
    pub atlas: String,
    pub data: Vec<Vec<TileData>>,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsVec2 {
    pub x: u32,
    pub y: u32,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsTransform {
    pub pos: MpsVec3,
    pub rot: MpsEuler,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsVec3 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsEuler {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

pub type CubeMap = [String; 6];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TileData {
    #[serde(flatten)]
    pub height: TileHeight,
    pub connections: ConnectionMap,
    pub material: MpsMaterial,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup: Option<PopupType>,
    #[serde(default)]
    pub walk_over: bool,
    pub wall_material: WallMaterialMap,
    pub silver_star_spawnable: bool,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TileHeight {
    Flat {
        ramp: MustBe!(false),
        height: f64,
    },
    Ramp {
        ramp: MustBe!(true),
        height: TileRamp,
    },
}

impl Default for TileHeight {
    fn default() -> Self {
        Self::Flat {
            ramp: MustBeBool,
            height: 0.0,
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct TileRamp {
    pub dir: TileRampDirection,
    pub pos: f64,
    pub neg: f64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TileRampDirection {
    #[serde(rename = "h")]
    Horizontal,
    #[serde(rename = "v")]
    Vertical,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConnectionMap {
    #[serde(rename = "n")]
    pub north: Connection,
    #[serde(rename = "e")]
    pub east: Connection,
    #[serde(rename = "s")]
    pub south: Connection,
    #[serde(rename = "w")]
    pub west: Connection,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Connection {
    Unconditional(bool),
    Conditional(ConnectionCondition),
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionCondition {
    Lock,
}

impl Default for Connection {
    fn default() -> Self {
        Self::Unconditional(false)
    }
}

#[serde_as]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WallMaterialMap {
    #[serde(rename = "n")]
    #[serde_as(as = "OneOrMany<_>")]
    pub north: Vec<MpsMaterial>,

    #[serde(rename = "e")]
    #[serde_as(as = "OneOrMany<_>")]
    pub east: Vec<MpsMaterial>,

    #[serde(rename = "s")]
    #[serde_as(as = "OneOrMany<_>")]
    pub south: Vec<MpsMaterial>,

    #[serde(rename = "w")]
    #[serde_as(as = "OneOrMany<_>")]
    pub west: Vec<MpsMaterial>,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpsMaterial(AtlasCoordValue);

type AtlasCoordValue = u8;
const ATLAS_SIZE: (AtlasCoordValue, AtlasCoordValue) = (16, 16);

impl MpsMaterial {
    /// Return value: `(u1, v1, u2, v2)`
    pub fn to_uv_coords(self) -> (f32, f32, f32, f32) {
        let u = (self.0 % ATLAS_SIZE.0) as f32 / ATLAS_SIZE.0 as f32;
        let v = (self.0 / ATLAS_SIZE.0) as f32 / ATLAS_SIZE.1 as f32;
        (
            u,
            v,
            u + 1.0 / ATLAS_SIZE.0 as f32,
            v + 1.0 / ATLAS_SIZE.1 as f32,
        )
    }

    pub fn from_uv_coords(u: f32, v: f32) -> Self {
        let x = (u * ATLAS_SIZE.0 as f32) as AtlasCoordValue;
        let y = (v * ATLAS_SIZE.1 as f32) as AtlasCoordValue;
        Self(y * ATLAS_SIZE.0 + x)
    }
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PopupType {
    #[default]
    LuckySpace,
    #[serde(rename = "shop-1")]
    Shop1,
    #[serde(rename = "shop-2")]
    Shop2,
    #[serde(rename = "star-1")]
    Star1,
    #[serde(rename = "star-2")]
    Star2,
    StarSteal,
}
