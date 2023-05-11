use crate::{Error, GraphUpdate, Node};

use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Debug)]
pub enum Command {
    Insert {
        id: Uuid,
        node: Node,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    Remove {
        id: Uuid,
        callback: oneshot::Sender<Result<(), Error>>,
    },
    Get {
        id: Uuid,
        callback: oneshot::Sender<Result<Node, Error>>,
    },
    List {
        callback: oneshot::Sender<Vec<(Uuid, Node)>>,
    },
    Subscribe {
        id: Uuid,
        subscriber: mpsc::Sender<GraphUpdate>,
        callback: oneshot::Sender<Result<Uuid, Error>>,
    },
    Unsubscribe {
        id: Uuid,
        callback: oneshot::Sender<Result<(), Error>>,
    },
}
