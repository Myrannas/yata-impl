use crate::block::{Block, Item};
use crate::delete_set::DeleteSet;
use crate::document::{BlockId, ClientId, Clock};
use crate::Document;
use std::ops::Range;

use crate::update::MergeResult::{Merged, NotMerged};
use bincode::de::Decoder;
use bincode::enc::Encoder;
use bincode::error::{DecodeError, EncodeError};
use bincode::{Decode, Encode};

#[derive(Eq, PartialEq, Debug, Clone, Encode, Decode)]
pub(crate) enum Content<T: Item> {
    Value(Vec<T>),
    Deleted(u64),
}
impl<T: Item> Content<T> {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Content::Value(mut value), Content::Value(value2)) => {
                value.extend(value2);

                Content::Value(value)
            }
            (Content::Value(value), _) | (_, Content::Value(value)) => Content::Value(value),
            (Content::Deleted(deleted), Content::Deleted(deleted2)) => {
                Content::Deleted(deleted + deleted2)
            }
        }
    }
}

enum MergeResult<T> {
    Merged(T),
    NotMerged(T, T),
}

#[derive(Eq, PartialEq, Debug, Clone, Encode, Decode)]
pub struct UpdateBlock<T: Item> {
    pub(crate) origin_left: Option<BlockId>,
    pub(crate) origin_right: Option<BlockId>,
    pub(crate) value: Content<T>,
}

impl<T: Item> UpdateBlock<T> {
    fn hydrate(self, id: Clock) -> Block<T> {
        let (length, value) = match self.value {
            Content::Value(value) => (value.len(), value),
            Content::Deleted(size) => (size as usize, vec![]),
        };

        Block {
            id,
            origin_left: self.origin_left,
            origin_right: self.origin_right,
            value,
            left: None,
            right: None,
            deleted: false,
            length,
        }
    }

    fn from_block(block: &Block<T>) -> UpdateBlock<T> {
        let value = if block.deleted {
            Content::Value(block.value.clone())
        } else {
            Content::Deleted(block.length as u64)
        };

        UpdateBlock {
            origin_left: block.origin_left.clone(),
            origin_right: block.origin_right.clone(),
            value,
        }
    }

    fn with_value(left: Option<BlockId>, right: Option<BlockId>, value: T) -> UpdateBlock<T> {
        UpdateBlock {
            origin_left: left,
            origin_right: right,
            value: Content::Value(vec![value]),
        }
    }

    fn length(&self) -> u64 {
        match &self.value {
            Content::Value(v) => v.len() as u64,
            Content::Deleted(v) => *v,
        }
    }

    fn try_merge(
        self,
        self_id: BlockId,
        other: Self,
        other_id: BlockId,
    ) -> MergeResult<UpdateBlock<T>> {
        let can_merge = if self.origin_right == other.origin_right
            && Some(self_id) == other.origin_left
            && self_id.client_id == other_id.client_id
            && self_id.clock + self.length() == other_id.clock
        {
            matches!(
                (&self.value, &other.value),
                (Content::Value(_), Content::Value(_)) | (Content::Deleted(_), Content::Deleted(_))
            )
        } else {
            false
        };

        if can_merge {
            Merged(UpdateBlock {
                origin_left: self.origin_left,
                origin_right: self.origin_right,
                value: self.value.merge(other.value),
            })
        } else {
            NotMerged(self, other)
        }
    }
}

impl<T: Item> From<Block<T>> for UpdateBlock<T> {
    fn from(block: Block<T>) -> Self {
        UpdateBlock {
            origin_left: block.origin_left,
            origin_right: block.origin_right,
            value: Content::Value(block.value),
        }
    }
}

#[derive(Eq, PartialEq, Clone, Encode, Decode, Debug)]
pub struct Update<T: Item> {
    dependency: Vec<(ClientId, Range<Clock>)>,
    blocks: Vec<(ClientId, Vec<UpdateBlock<T>>)>,
    deletes: DeleteSet,
}

#[derive(PartialEq, Debug)]
enum ValidationError {
    ClientDoesNotExist(ClientId),
    UpdateOutsideRange(BlockId),
    InvalidUpdateRange(ClientId),
}

impl<T: Item> Update<T> {
    pub fn from_document(document: &Document<T>) -> Update<T> {
        let blocks = document
            .store
            .data
            .iter()
            .map(|(client_id, block)| {
                (*client_id, block.iter().map(|u| u.clone().into()).collect())
            })
            .collect();

        Update {
            blocks,
            dependency: document
                .store
                .data
                .iter()
                .map(|(key, value)| (*key, 0..(value.len() as Clock)))
                .collect(),
            deletes: DeleteSet::from(document),
        }
    }

    pub fn apply(self, document: &mut Document<T>) -> Result<(), ()> {
        // Check dependencies
        for (client_id, dependency_range) in self.dependency {
            let start = document.clients.get(&client_id).unwrap_or(&0);

            if dependency_range.start > *start {
                return Err(());
            }
        }

        for (client_id, blocks) in self.blocks.into_iter() {
            let hydrated_blocks = blocks
                .into_iter()
                .enumerate()
                .map(|(i, block)| block.hydrate(i as Clock))
                .collect();

            document.store.integrate(client_id, hydrated_blocks)
        }

        Ok(())
    }

    pub(crate) fn from_blocks(
        client_id: ClientId,
        blocks: Vec<Block<T>>,
        dependency: Vec<(ClientId, Range<Clock>)>,
    ) -> Update<T> {
        let update_blocks = blocks.into_iter().map(|f| f.into()).collect();

        Update {
            blocks: vec![(client_id, update_blocks)],
            dependency,
            deletes: DeleteSet::empty(),
        }
    }

    fn validate(&self) -> Result<(), ValidationError> {
        for (client, blocks) in &self.blocks {
            for block in blocks {
                if !self.does_clock_exist(block.origin_left) {
                    return Err(ValidationError::UpdateOutsideRange(
                        block.origin_left.unwrap(),
                    ));
                }

                if !self.does_clock_exist(block.origin_right) {
                    return Err(ValidationError::UpdateOutsideRange(
                        block.origin_right.unwrap(),
                    ));
                }
            }

            if let Some(range) = self.get_version_range(*client) {
                if (range.end - range.start) != (blocks.len() as Clock) {
                    return Err(ValidationError::InvalidUpdateRange(*client));
                }
            } else {
                return Err(ValidationError::ClientDoesNotExist(*client));
            }
        }

        Ok(())
    }

    fn get_version_range(&self, client_id: ClientId) -> Option<Range<Clock>> {
        self.dependency
            .iter()
            .find(|(cid, ..)| *cid == client_id)
            .map(|(cid, range)| range.clone())
    }

    fn does_clock_exist(&self, block: Option<BlockId>) -> bool {
        if let Some(block) = block {
            if let Some(range) = self.get_version_range(block.client_id) {
                if block.clock > range.end {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    fn compact(self) -> Self {
        self
        // for (client, block) in self.blocks {
        //     let mut start = self.get_version_range(*client).unwrap().start;
        //     let output: Vec<UpdateBlock<T>> = Vec::with_capacity(block.len());
        //
        //     block
        //         .into_iter()
        //         .fold((None, output), |(prev, mut output), next| {
        //             let next_id = BlockId::new(client, start);
        //             start += 1;
        //             match prev {
        //                 None => (Some((next, next_id)), output),
        //                 Some((prev, prev_id)) => match prev.try_merge(prev_id, next, next_id) {
        //                     Merged(value) => (Some((value, next_id)), output),
        //                     NotMerged(value1, value2) => {
        //                         output.push(value1);
        //                         (Some((value2, next_id)), output)
        //                     }
        //                 },
        //             }
        //         })
        // }
    }
}

#[cfg(test)]
mod tests {
    use crate::block::Block;
    use crate::delete_set::DeleteSet;
    use crate::document::BlockId;
    use crate::update::{Update, UpdateBlock, ValidationError};
    use crate::Document;
    use bincode::{config, decode_from_slice, encode_to_vec};

    #[test]
    fn can_create_update_from_document() {
        let mut doc = Document::new();
        doc.store.append("test".to_owned());

        let document_as_update = Update::from_document(&doc);

        assert_eq!(
            document_as_update.blocks[0],
            (
                doc.client_id,
                vec![UpdateBlock::with_value(None, None, "test".to_owned())]
            )
        );
    }

    #[test]
    fn can_apply_update_to_document() {
        let mut doc = Document::new();
        doc.store.append("test".to_owned());

        let update = Update::from_document(&doc);

        let mut doc2 = Document::new();
        update.apply(&mut doc2);

        let data: Vec<&String> = doc2.store.iter_values().collect();

        assert_eq!(data, vec!["test"]);
    }

    #[test]
    fn cant_apply_update_with_missing_dependency() {
        let mut doc = Document::with_client_id(1);
        doc.store.append("test".to_owned());

        let update = Update::from_blocks(2, vec![], vec![(3, 2..3)]);

        let result = update.apply(&mut doc);

        assert_eq!(result, Err(()))
    }

    #[test]
    fn can_merge_two_documents() {
        let mut doc = Document::with_client_id(1);
        doc.store.append("test".to_owned());

        let update = Update::from_document(&doc);

        let mut doc2 = Document::with_client_id(2);
        doc2.store.append("test2".to_owned());
        update.apply(&mut doc2);

        let data: Vec<&String> = doc2.store.iter_values().collect();

        assert_eq!(data, vec!["test", "test2"]);
    }

    #[test]
    fn can_validate_empty_doc() {
        let valid_update: Update<String> = Update {
            blocks: vec![],
            dependency: vec![],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(valid_update.validate(), Ok(()));
    }

    #[test]
    fn throws_error_if_client_doesnt_exist_range() {
        let valid_update: Update<String> = Update {
            blocks: vec![(1, vec![])],
            dependency: vec![],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(
            valid_update.validate(),
            Err(ValidationError::ClientDoesNotExist(1))
        );
    }

    #[test]
    fn throws_error_if_update_client_doesnt_exist_range() {
        let valid_update: Update<String> = Update {
            blocks: vec![(
                1,
                vec![UpdateBlock::with_value(
                    Some(BlockId::new(2, 0)),
                    None,
                    "Test".to_owned(),
                )],
            )],
            dependency: vec![(1, 0..1)],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(
            valid_update.validate(),
            Err(ValidationError::UpdateOutsideRange(BlockId::new(2, 0)))
        );
    }

    #[test]
    fn throws_error_if_update_outside_range_left() {
        let valid_update: Update<String> = Update {
            blocks: vec![(
                1,
                vec![UpdateBlock::with_value(
                    Some(BlockId::new(2, 1)),
                    None,
                    "Test".to_owned(),
                )],
            )],
            dependency: vec![(1, 0..1), (2, 0..0)],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(
            valid_update.validate(),
            Err(ValidationError::UpdateOutsideRange(BlockId::new(2, 1)))
        );
    }

    #[test]
    fn throws_error_if_update_outside_range_right() {
        let valid_update: Update<String> = Update {
            blocks: vec![(
                1,
                vec![UpdateBlock::with_value(
                    None,
                    Some(BlockId::new(2, 1)),
                    "Test".to_owned(),
                )],
            )],
            dependency: vec![(1, 0..1), (2, 0..0)],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(
            valid_update.validate(),
            Err(ValidationError::UpdateOutsideRange(BlockId::new(2, 1)))
        );
    }

    #[test]
    fn validate_ok_if_valid_update() {
        let valid_update: Update<String> = Update {
            blocks: vec![(
                1,
                vec![UpdateBlock::with_value(
                    Some(BlockId::new(2, 0)),
                    None,
                    "Test".to_owned(),
                )],
            )],
            dependency: vec![(1, 0..1), (2, 0..0)],
            deletes: DeleteSet::empty(),
        };

        assert_eq!(valid_update.validate(), Ok(()));
    }

    #[test]
    fn validate_apply_serialized_update() {
        let mut document = Document::with_client_id(1);
        document.store.append("Test".to_owned());
        document.store.append("Test 2".to_owned());
        document.store.append("Test 3".to_owned());
        document.store.delete_range(0, 2);

        let update = Update::from_document(&document);

        let configuration = config::standard();
        let encoded_update = encode_to_vec(update.clone(), configuration).unwrap();
        let (decoded_update, size): (Update<String>, usize) =
            decode_from_slice(&encoded_update, configuration).unwrap();

        assert_eq!(update, decoded_update);
        assert_eq!(encoded_update.len(), 37);
    }
}
