use crate::{node::Body, AddNode, InputNode, MultiplyNode, Node, RewireNode};

use uuid::Uuid;

pub fn to_proto(id: &Uuid, node: &cbmix_graph::Node) -> Node {
    Node {
        id: Some(id.to_string()),
        body: Some(match node {
            cbmix_graph::Node::Input { channels } => Body::Input(InputNode {
                channels: channels.clone().into(),
            }),
            cbmix_graph::Node::Add { a, b } => Body::Add(AddNode {
                a: a.map(|u| u.to_string()),
                b: b.map(|u| u.to_string()),
            }),
            cbmix_graph::Node::Multiply { a, b } => Body::Multiply(MultiplyNode {
                a: a.map(|u| u.to_string()),
                b: b.map(|u| u.to_string()),
            }),
            cbmix_graph::Node::Rewire { input, map } => Body::Rewire(RewireNode {
                input: input.map(|u| u.to_string()),
                map: map.iter().map(|i| *i as i32).collect(),
            }),
        }),
    }
}

pub fn from_proto(node: &Node) -> Option<(Option<Uuid>, cbmix_graph::Node)> {
    if let Some(body) = node.body.clone() {
        let body = match body {
            Body::Input(InputNode { channels }) => cbmix_graph::Node::Input {
                channels: channels.try_into().ok()?,
            },
            Body::Add(AddNode { a, b }) => cbmix_graph::Node::Add {
                a: a.and_then(|s| Uuid::try_parse(&s).ok()),
                b: b.and_then(|s| Uuid::try_parse(&s).ok()),
            },
            Body::Multiply(MultiplyNode { a, b }) => cbmix_graph::Node::Multiply {
                a: a.and_then(|s| Uuid::try_parse(&s).ok()),
                b: b.and_then(|s| Uuid::try_parse(&s).ok()),
            },
            Body::Rewire(RewireNode { input, map }) => cbmix_graph::Node::Rewire {
                input: input.and_then(|s| Uuid::try_parse(&s).ok()),
                map: map
                    .iter()
                    .map(|i| *i as u16)
                    .collect::<Vec<u16>>()
                    .try_into()
                    .ok()?,
            },
        };

        Some((node.id.as_ref().and_then(|u| Uuid::try_parse(u).ok()), body))
    } else {
        None
    }
}
