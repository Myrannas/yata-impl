use crate::store::Store;
use std::collections::HashMap;

use crate::block::Item;
use bincode::{Decode, Encode};

pub type Clock = u64;
pub type ClientId = u64;

pub type ClockVector = HashMap<ClientId, Clock>;

#[derive(Debug, Eq, PartialEq, Clone, Copy, Encode, Decode)]
pub struct BlockId {
    pub client_id: ClientId,
    pub clock: Clock,
}

impl BlockId {
    pub fn new(client_id: ClientId, clock: Clock) -> BlockId {
        BlockId { client_id, clock }
    }
}

#[derive(Debug)]
pub struct Document<T: Item> {
    clock: Clock,
    pub(crate) client_id: ClientId,
    pub(crate) clients: ClockVector,
    pub(crate) store: Store<T>,
}

impl<T: Item> Document<T> {
    pub(crate) fn with_client_id(client_id: u64) -> Document<T> {
        Document {
            clock: 0,
            client_id,
            clients: HashMap::new(),
            store: Store::new(client_id),
        }
    }

    pub fn new() -> Document<T> {
        Document::with_client_id(rand::random())
    }
}
