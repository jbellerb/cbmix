use crate::Error;

use cbmix_admin_proto::{SceneId, SceneUpdateEvent};
use ola::DmxBuffer;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug)]
pub enum Event {
    Subscribe(Uuid, mpsc::Sender<DmxBuffer>),
    CreateInput(Uuid, DmxBuffer, oneshot::Sender<Result<(), Error>>),
    CreateOutput(Uuid, Uuid, oneshot::Sender<Result<(), Error>>),
    SceneUpdate(SceneUpdateEvent),
    SceneSwitch(SceneId),
}
