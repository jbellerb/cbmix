use crate::proto::cbmix::{SceneId, SceneUpdateEvent};

use ola::DmxBuffer;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum Event {
    Subscribe(Uuid, mpsc::Sender<DmxBuffer>),
    SceneUpdate(SceneUpdateEvent),
    SceneSwitch(SceneId),
}
