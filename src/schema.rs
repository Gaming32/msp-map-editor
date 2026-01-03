use crate::utils::grid_as_vec_vec;
use enum_map::{Enum, EnumMap};
use grid::{Grid, grid};
use monostate::{MustBe, MustBeBool};
use serde::{Deserialize, Serialize};
use serde_with::OneOrMany;
use serde_with::serde_as;
use strum::IntoStaticStr;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapFile {
    pub starting_tile: MpsVec2,
    pub shop_warp_tiles: Vec<MpsVec2>,
    pub star_warp_tile: MpsVec2,
    pub podium_position: MpsVec2,
    pub results_anim_cam_poses: Vec<MpsVec3>,
    pub tutorial_star: MpsTransform,
    pub tutorial_shop: MpsTransform,
    #[serde(flatten)]
    pub textures: Textures<String>,
    pub shops: EnumMap<ShopNumber, Vec<ShopItem>>,
    #[serde(with = "grid_as_vec_vec")]
    pub data: Grid<TileData>,
}

impl Default for MapFile {
    fn default() -> Self {
        Self {
            starting_tile: Default::default(),
            shop_warp_tiles: Default::default(),
            star_warp_tile: Default::default(),
            podium_position: Default::default(),
            results_anim_cam_poses: Default::default(),
            tutorial_star: Default::default(),
            tutorial_shop: Default::default(),
            textures: Default::default(),
            shops: Default::default(),
            data: grid![[TileData::default()]],
        }
    }
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsVec2 {
    pub x: i32,
    pub y: i32,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsTransform {
    pub pos: MpsVec3,
    pub rot: MpsVec3,
}

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct MpsVec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Textures<T> {
    pub skybox: CubeMap<T>,
    pub dark_skybox: CubeMap<T>,
    pub atlas: T,
}

pub type CubeMap<T> = [T; 6];

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Enum)]
#[serde(rename_all = "kebab-case")]
pub enum ShopNumber {
    #[default]
    #[serde(rename = "shop-1")]
    Shop1,
    #[serde(rename = "shop-2")]
    Shop2,
    #[serde(rename = "shop-3")]
    Shop3,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, IntoStaticStr)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ShopItem {
    #[default]
    DoubleDice,
    TripleDice,
    Pipe,
    GoldPipe,
    CustomDice,
    Tacticooler,
    ShopHopBox,
    InkJet,
    Key,
}

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

impl TileHeight {
    pub fn center_height(&self) -> f64 {
        match self {
            Self::Flat { height, .. } => *height,
            Self::Ramp { height, .. } => (height.pos + height.neg) / 2.0,
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Default for WallMaterialMap {
    fn default() -> Self {
        Self {
            north: vec![Default::default()],
            east: vec![Default::default()],
            south: vec![Default::default()],
            west: vec![Default::default()],
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MpsMaterial(AtlasCoordValue);

type AtlasCoordValue = u8;
const ATLAS_SIZE: (AtlasCoordValue, AtlasCoordValue) = (16, 16);

impl MpsMaterial {
    /// Return value: `(u1, v1, u2, v2)`
    pub fn to_uv_coords(self) -> (f32, f32, f32, f32) {
        let u = (self.0 % ATLAS_SIZE.0) as f32 / ATLAS_SIZE.0 as f32;
        let v = (ATLAS_SIZE.1 - 1 - self.0 / ATLAS_SIZE.0) as f32 / ATLAS_SIZE.1 as f32;
        (
            u,
            v,
            u + 1.0 / ATLAS_SIZE.0 as f32,
            v + 1.0 / ATLAS_SIZE.1 as f32,
        )
    }

    pub fn from_uv_coords(u: f32, v: f32) -> Self {
        let x = (u * ATLAS_SIZE.0 as f32) as AtlasCoordValue;
        let y = ATLAS_SIZE.1 - 1 - (v * ATLAS_SIZE.1 as f32) as AtlasCoordValue;
        Self(y * ATLAS_SIZE.0 + x)
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PopupType {
    #[default]
    LuckySpace,
    #[serde(rename = "star-1")]
    Star1,
    #[serde(rename = "star-2")]
    Star2,
    StarSteal,
    #[serde(untagged)]
    Shop(ShopNumber),
}
