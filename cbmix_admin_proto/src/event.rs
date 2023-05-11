use crate::message::{Message, MessageType};
use crate::SubscriptionUpdateEvent;

use prost::Message as ProstMessage;

pub trait Event: ProstMessage + Sized {
    const NAME: &'static str;

    fn to_message(&self) -> Message {
        Message {
            r#type: MessageType::Event as i32,
            seq: None,
            name: Some(Self::NAME.to_string()),
            body: Some(self.encode_to_vec()),
        }
    }
}

impl Event for SubscriptionUpdateEvent {
    const NAME: &'static str = "SubscriptionUpdateEvent";
}
