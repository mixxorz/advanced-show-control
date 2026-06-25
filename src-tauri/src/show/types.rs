use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowDocument {
    pub lockout: bool,
    pub scene_configs: Vec<crate::scenes::SceneConfig>,
    pub cued_scene_internal_id: Option<uuid::Uuid>,
}

impl ShowDocument {
    pub fn empty() -> Self {
        Self {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_internal_id: None,
        }
    }
}
