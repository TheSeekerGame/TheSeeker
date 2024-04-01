use bevy::reflect::TypePath;

use crate::prelude::*;

/// Arbitrary config asset type
///
/// Would typically be loaded from TOML files.
#[derive(Asset, Debug, Clone)]
#[derive(Serialize, Deserialize)]
#[derive(TypePath)]
#[serde(transparent)]
pub struct DynamicConfig(pub HashMap<String, DynamicConfigValue>);

#[derive(Debug, Clone, PartialEq)]
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum DynamicConfigValue {
    Int(i64),
    Float(f64),
    String(String),
}
