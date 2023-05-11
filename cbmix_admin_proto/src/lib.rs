mod entity;
pub mod event;
pub mod message;

use entity::to_proto;
use message::{Message, MessageType};

use prost::Message as ProstMessage;
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

pub fn error_message(seq: u32, text: String) -> Message {
    Message {
        r#type: MessageType::ResponseError as i32,
        seq: Some(seq),
        name: None,
        body: Some(text.into_bytes()),
    }
}

pub enum GraphServiceRequest {
    Subscribe(Uuid),
    Unsubscribe(Uuid),
    GetNode(Uuid),
    GetNodes,
    UpdateNode(Option<Uuid>, cbmix_graph::Node),
    RemoveNode(Uuid),
}

pub enum GraphServiceResponse {
    Subscribe(Uuid),
    Unsubscribe,
    GetNode(Uuid, cbmix_graph::Node),
    GetNodes(Vec<(Uuid, cbmix_graph::Node)>),
    UpdateNode(Uuid),
    RemoveNode,
}

impl GraphServiceResponse {
    pub fn to_message(&self, seq: u32) -> Message {
        let (name, body) = match self {
            GraphServiceResponse::Subscribe(id) => (
                "Subscribe",
                Some(SubscriptionId { id: id.to_string() }.encode_to_vec()),
            ),
            GraphServiceResponse::Unsubscribe => ("Unsubscribe", None),
            GraphServiceResponse::GetNode(id, node) => {
                ("GetNode", Some(to_proto(id, node).encode_to_vec()))
            }
            GraphServiceResponse::GetNodes(nodes) => (
                "GetNodes",
                Some(
                    Nodes {
                        nodes: nodes
                            .iter()
                            .map(|(i, n)| to_proto(i, n))
                            .collect::<Vec<Node>>(),
                    }
                    .encode_to_vec(),
                ),
            ),
            GraphServiceResponse::UpdateNode(id) => (
                "UpdateNode",
                Some(NodeId { id: id.to_string() }.encode_to_vec()),
            ),
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
