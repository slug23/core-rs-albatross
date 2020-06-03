use futures::sync::mpsc::{unbounded, SendError, UnboundedReceiver, UnboundedSender};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{Message, MessageType};

#[derive(Default)]
pub struct MessageDistributor {
    sinks: HashMap<MessageType, Vec<Arc<UnboundedSender<Message>>>>,
}

impl MessageDistributor {
    pub fn subscribe(&mut self, msg_types: &[MessageType]) -> UnboundedReceiver<Message> {
        let (sender, receiver) = unbounded();
        let sender = Arc::new(sender);
        for msg_type in msg_types {
            self.sinks
                .entry(*msg_type)
                .or_default()
                .push(Arc::clone(&sender));
        }
        receiver
    }

    pub fn notify(&self, msg: Message) -> Result<(), SendError<Message>> {
        let ty = msg.ty();
        if let Some(senders) = self.sinks.get(&ty) {
            for sender in senders.iter().take(senders.len() - 1) {
                let msg_cloned = msg.clone();
                sender.unbounded_send(msg_cloned)?;
            }
            if let Some(last_sender) = senders.last() {
                last_sender.unbounded_send(msg)?;
            }
        }
        Ok(())
    }

    pub fn notify_and_cleanup(&mut self, msg: Message) {
        let ty = msg.ty();
        if let Some(senders) = self.sinks.get_mut(&ty) {
            let mut to_remove = vec![];
            for (i, sender) in senders.iter().take(senders.len() - 1).enumerate() {
                let msg_cloned = msg.clone();
                if sender.unbounded_send(msg_cloned).is_err() {
                    to_remove.push(i);
                }
            }
            if let Some(last_sender) = senders.last() {
                if last_sender.unbounded_send(msg).is_err() {
                    to_remove.push(senders.len() - 1);
                }
            }

            for (offset, &i) in to_remove.iter().enumerate() {
                senders.remove(i - offset);
            }
        }
    }
}
