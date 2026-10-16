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
impl DynamicConfigValue {
    pub fn as_float(&self) -> Result<f64, String> {
        match self {
            DynamicConfigValue::Float(value) => Ok(*value),
            other => Err(format!(
                "Expected float value, but found {other:?}"
            )),
        }
    }

}

/// A utility type for loading config values. Use like:
///
/// ```
/// use std::collections::HashMap;
/// use theseeker_engine::assets::config::{DynamicConfigValue, update_field};
///
/// struct PlayerConfig {
///     jump_vel_init: f32,
/// }
///
/// let mut errors = Vec::new();
/// let mut dynamic_cfg = HashMap::new();
/// dynamic_cfg.insert("jump_vel_init".to_owned(), DynamicConfigValue::Float(12.0));
/// let mut struct_config = PlayerConfig { jump_vel_init: 0.0 };
///
/// update_field(&mut errors, &dynamic_cfg, "jump_vel_init", |val| {
///     struct_config.jump_vel_init = val;
/// });
///
/// assert!(errors.is_empty());
/// assert_eq!(struct_config.jump_vel_init, 12.0);
/// ```
pub fn update_field<F>(
    errors: &mut Vec<String>,
    config: &HashMap<String, DynamicConfigValue>,
    field: &str,
    setter: F,
) where
    F: FnOnce(f32),
{
    if let Some(value) = config.get(field) {
        match value.as_float() {
            Ok(val) => setter(val as f32),
            Err(err) => errors.push(format!(
                "Error parsing '{field}': {err}"
            )),
        }
    } else {
        errors.push(format!(
            "Missing '{field}' field in the config"
        ));
    }
}
