use serde::{Deserialize, Serialize};

/// Which dialogs are open
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Windows {
    pub new_graph: bool,
    pub export: bool,
    /// The "Save 3D view image" dialog
    pub view_image: bool,
    pub about: bool,
    pub preferences: bool,
    pub shortcuts: bool,
    pub home: bool,
    #[serde(skip_deserializing)]
    pub unsaved: Option<crate::unsaved::Guarded>,
}
