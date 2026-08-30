//! Runtime asset executable document hydration.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssetDoc {
    pub(super) id: String,
    media_type: String,
    logical_path: String,
    digest: String,
}
impl AssetDoc {
    pub(super) fn into_runtime(self) -> Result<RuntimeAssetDefinition, ProgramError> {
        if !safe_asset_path(&self.logical_path) {
            return Err(ProgramError::InvalidAssetPath(self.logical_path));
        }
        if self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(ProgramError::InvalidAssetDigest(self.digest));
        }
        Ok(RuntimeAssetDefinition {
            id: AssetId::new(self.id),
            media_type: self.media_type,
            logical_path: self.logical_path,
            digest: self.digest,
        })
    }
}
