use crate::{node::Body, InputNode, Node};

use uuid::Uuid;

pub fn to_proto(id: &Uuid, node: &cbmix_graph::Node) -> Node {
    match node {
        cbmix_graph::Node::Input { channels } => Node {
            id: Some(id.to_string()),
            body: Some(Body::Input(InputNode {
                channels: channels.clone().into(),
            })),
        },
    }
}

pub fn from_proto(node: &Node) -> Option<(Option<Uuid>, cbmix_graph::Node)> {
    if let Some(body) = node.body.clone() {
        let body = match body {
            Body::Input(InputNode { channels }) => cbmix_graph::Node::Input {
                channels: channels.try_into().ok()?,
            },
        };

        Some((node.id.as_ref().and_then(|u| Uuid::try_parse(u).ok()), body))
    } else {
        None
    }
}
