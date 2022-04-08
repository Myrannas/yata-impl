use crate::store::Store;
use std::collections::HashMap;

pub type Clock = u64;
pub type ClientId = u64;

pub type ClockVector = HashMap<ClientId, Clock>;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
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
pub struct Document<T: Clone> {
    clock: Clock,
    pub(crate) client_id: ClientId,
    clients: ClockVector,
    pub(crate) store: Store<T>,
}

impl<T: Clone> Document<T> {
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Block<T: Clone> {
    pub(crate) id: Clock,
    pub(crate) origin_left: Option<BlockId>,
    pub(crate) left: Option<BlockId>,
    // left neighbor at moment of original insertion
    pub(crate) origin_right: Option<BlockId>,
    pub(crate) right: Option<BlockId>,
    // right neighbor at moment of original insertion
    pub(crate) value: Option<T>,
    pub(crate) deleted: bool,
}

impl<T: Clone> Block<T> {
    pub fn with_value(id: Clock, left: Option<BlockId>, value: T) -> Block<T> {
        Block {
            id,
            origin_left: left,
            left,
            origin_right: None,
            right: None,
            value: Some(value),
            deleted: false,
        }
    }

    pub fn with_value_and_right(
        id: Clock,
        left: Option<BlockId>,
        right: Option<BlockId>,
        value: T,
    ) -> Block<T> {
        Block {
            id,
            origin_left: left,
            origin_right: right,
            left,
            right,
            value: Some(value),
            deleted: false,
        }
    }
}
