pub mod message;

use message::{Message, MessageType};
use node::Body;

use prost::Message as MessageTrait;
use thiserror::Error;
use uuid::Uuid;

include!(concat!(env!("OUT_DIR"), "/cbmix.rs"));

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unable to decode protobuf message")]
    Decode,
    #[error("Received unexpected message type")]
    UnexpectedType,
    #[error("Received unknown request method")]
    UnknownMethod,
    #[error("Received incomplete event")]
    IncompleteEvent,
    #[error("Failed to parse UUID")]
    Uuid,
}

pub enum GraphServiceRequest {
    Subscribe(Uuid),
    Unsubscribe(Uuid),
    GetNode(Uuid),
    GetNodes,
    UpdateNode(Uuid, Body),
    RemoveNode(Uuid),
}

pub enum GraphServiceResponse {
    Subscribe(OutputUpdateEvent),
    Unsubscribe,
    GetNode(Node),
    GetNodes(Nodes),
    UpdateNode(NodeId),
    RemoveNode,
}

impl GraphServiceResponse {
    pub fn to_message(&self, seq: u32) -> Message {
        let (name, body) = match self {
            GraphServiceResponse::Subscribe(update) => ("Subscribe", Some(update.encode_to_vec())),
            GraphServiceResponse::Unsubscribe => ("Unsubscribe", None),
            GraphServiceResponse::GetNode(node) => ("GetNode", Some(node.encode_to_vec())),
            GraphServiceResponse::GetNodes(nodes) => ("GetNodes", Some(nodes.encode_to_vec())),
            GraphServiceResponse::UpdateNode(node_id) => {
                ("UpdateNode", Some(node_id.encode_to_vec()))
            }
            GraphServiceResponse::RemoveNode => ("RemoveNode", None),
        };

        Message {
            r#type: MessageType::Response as i32,
            seq: Some(seq),
            name: Some(name.to_string()),
            body,
        }
    }
}
