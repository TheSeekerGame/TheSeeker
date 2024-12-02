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
            other => {
                Err(format!(
                    "Expected float value, but found {:?}",
                    other
                ))
            },
        }
    }

    pub fn as_int(&self) -> Result<i64, String> {
        match self {
            DynamicConfigValue::Int(value) => Ok(*value),
            other => {
                Err(format!(
                    "Expected integer value, but found {:?}",
                    other
                ))
            },
        }
    }

    pub fn as_string(&self) -> Result<&str, String> {
        match self {
            DynamicConfigValue::String(value) => Ok(value),
            other => {
                Err(format!(
                    "Expected string value, but found {:?}",
                    other
                ))
            },
        }
    }
}

/// A utility type for loading config values. Use like:
///
/// ```
/// let mut errors = Vec::new();
/// update_field(&mut errors, &dynamic_cfg.0, "jump_vel_init", |val| struct_config.jump_vel_init = val);
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
            Err(err) => {
                errors.push(format!(
                    "Error parsing '{}': {}",
                    field, err
                ))
            },
        }
    } else {
        errors.push(format!(
            "Missing '{}' field in the config",
            field
        ));
    }
}
