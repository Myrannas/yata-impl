use crate::document::{BlockId, ClientId, Clock};
use std::ops::Add;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Block<T: Clone> {
    // The clock index this block was inserted as
    pub(crate) id: Clock,
    pub(crate) origin_left: Option<BlockId>,
    pub(crate) left: Option<BlockId>,
    // left neighbor at moment of original insertion
    pub(crate) origin_right: Option<BlockId>,
    pub(crate) right: Option<BlockId>,
    // right neighbor at moment of original insertion
    pub(crate) value: Vec<T>,
    pub(crate) length: usize,
    pub(crate) deleted: bool,
}

pub trait Item: Clone {}

impl<T: Item> Item for Option<T> {
    // fn split_at(self, index: Clock) -> (Self, Self) {
    //     match self {
    //         Some(value) => {
    //             let (left, right) = value.split_at(index);
    //             (Some(left), Some(right))
    //         }
    //         None => (None, None),
    //     }
    // }
    //
    // fn merge(self, right: Self) -> Self {
    //     match (self, right) {
    //         (Some(left), Some(right)) => Some(left.merge(right)),
    //         (Some(value), None) | (None, Some(value)) => Some(value),
    //         (None, None) => None,
    //     }
    // }
}

impl<T: Clone> Block<T> {
    pub(crate) fn delete(&mut self) {
        if !self.deleted {
            self.deleted = true;
            self.value = vec![];
        }
    }
}

impl<T: Item> Block<T> {
    pub fn with_value(id: Clock, left: Option<BlockId>, value: T) -> Block<T> {
        Block {
            id,
            origin_left: left,
            left,
            origin_right: None,
            right: None,
            value: vec![value],
            length: 1,
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
            value: vec![value],
            length: 1,
            deleted: false,
        }
    }

    pub fn merge_with_right(mut self, next: Block<T>) -> Block<T> {
        if !self.deleted {
            self.value.extend(next.value);
        }

        Block {
            id: self.id,
            origin_left: self.origin_left,
            origin_right: self.origin_right,
            left: self.left,
            right: self.right,
            value: self.value,
            length: self.length + next.length,
            deleted: self.deleted,
        }
    }

    pub fn split_at(self, client_id: ClientId, index: Clock) -> (Block<T>, Block<T>) {
        let (left_value, right_value) = if self.deleted {
            (vec![], vec![])
        } else {
            let (left, right) = self.value.split_at(index as usize);

            (left.to_vec(), right.to_vec())
        };

        let left_block_id = Some(BlockId::new(client_id, self.id));
        let right_block_id = Some(BlockId::new(client_id, self.id + index));

        (
            Block {
                id: self.id,
                origin_left: self.origin_left,
                origin_right: self.origin_right,
                left: self.left,
                right: right_block_id,
                value: left_value,
                length: index as usize,
                deleted: self.deleted,
            },
            Block {
                id: self.id + index,
                origin_left: left_block_id,
                origin_right: self.origin_right,
                left: left_block_id,
                right: self.right,
                value: right_value,
                length: self.length - index as usize,
                deleted: self.deleted,
            },
        )
    }
}

impl Item for String {}
