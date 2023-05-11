use crate::{node::Body, Error, GraphServiceRequest, Node, NodeId};

use prost::Message as MessageTrait;
use uuid::Uuid;

include!(concat!(env!("OUT_DIR"), "/cbmix.message.rs"));

impl Message {
    pub fn get_request(&self) -> Result<(u32, GraphServiceRequest), Error> {
        if self.r#type() != MessageType::Request {
            return Err(Error::UnexpectedType);
        }

        if let (Some(name), Some(seq)) = (self.name.as_ref(), self.seq) {
            match name.as_str() {
                "Subscribe" => Ok((
                    seq,
                    GraphServiceRequest::Subscribe(parse_node_id(
                        self.body.as_ref().ok_or(Error::IncompleteEvent)?,
                    )?),
                )),
                "Unsubscribe" => Ok((
                    seq,
                    GraphServiceRequest::Unsubscribe(parse_node_id(
                        self.body.as_ref().ok_or(Error::IncompleteEvent)?,
                    )?),
                )),
                "GetNode" => Ok((
                    seq,
                    GraphServiceRequest::GetNode(parse_node_id(
                        self.body.as_ref().ok_or(Error::IncompleteEvent)?,
                    )?),
                )),
                "GetNodes" => Ok((seq, GraphServiceRequest::GetNodes)),
                "UpdateNode" => {
                    let (id, body) =
                        parse_node(self.body.as_deref().ok_or(Error::IncompleteEvent)?)?;

                    Ok((seq, GraphServiceRequest::UpdateNode(id, body)))
                }
                "RemoveNode" => Ok((
                    seq,
                    GraphServiceRequest::GetNode(parse_node_id(
                        self.body.as_ref().ok_or(Error::IncompleteEvent)?,
                    )?),
                )),
                _ => Err(Error::UnknownMethod),
            }
        } else {
            Err(Error::IncompleteEvent)
        }
    }
}

fn parse_node(body: &[u8]) -> Result<(Uuid, Body), Error> {
    let node = Node::decode(body).map_err(|_| Error::Decode)?;
    let id = Uuid::try_parse(&node.id.ok_or(Error::IncompleteEvent)?).map_err(|_| Error::Uuid)?;
    let body = node.body.ok_or(Error::IncompleteEvent)?;

    Ok((id, body))
}

fn parse_node_id(body: &[u8]) -> Result<Uuid, Error> {
    let node_id = NodeId::decode(body).map_err(|_| Error::Decode)?;

    Uuid::try_parse(&node_id.id).map_err(|_| Error::Uuid)
}
