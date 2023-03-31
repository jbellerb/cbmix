use crate::proto::cbmix::{SceneId, SceneUpdateEvent};

#[derive(Clone, Debug)]
pub enum Event {
    SceneUpdate(SceneUpdateEvent),
    SceneSwitch(SceneId),
}
