use crate::block::{Block, Item};
use crate::document::{BlockId, ClientId, Clock};
use crate::Document;
use bincode::{Decode, Encode};

#[derive(Eq, PartialEq, Clone, Encode, Decode, Debug)]
pub struct DeleteSet {
    deletes: Vec<(ClientId, Vec<(Clock, usize)>)>,
}

impl DeleteSet {
    pub fn apply<T: Item>(&self, document: &mut Document<T>) {
        for (client, clocks) in &self.deletes {
            for (clock, length) in clocks {
                for i in *clock..(*clock + (*length as Clock)) {
                    document.store[BlockId::new(*client, i)].delete();
                }
            }
        }
    }

    pub fn from<T: Item>(document: &Document<T>) -> DeleteSet {
        DeleteSet {
            deletes: document
                .store
                .data
                .iter()
                .map(|(client_id, block)| {
                    (
                        *client_id,
                        block
                            .iter()
                            .filter(|Block { deleted, .. }| *deleted)
                            .map(|Block { deleted, id, .. }| (*id, 1))
                            .collect(),
                    )
                })
                .collect(),
        }
    }

    pub fn empty() -> DeleteSet {
        DeleteSet { deletes: vec![] }
    }
}
