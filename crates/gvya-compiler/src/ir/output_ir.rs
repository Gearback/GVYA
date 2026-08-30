//! Asset executable IR serialization.

use super::helpers::*;
use super::*;

pub(super) fn assets(project: &ComposedProject) -> JsonValue {
    JsonValue::Array(
        project
            .assets
            .values()
            .map(|row| {
                object([
                    ("id", string(row.id.as_str())),
                    ("media_type", string(&row.media_type)),
                    ("logical_path", string(&row.logical_path)),
                    ("digest", string(row.digest.as_str())),
                ])
            })
            .collect(),
    )
}
