use crate::document::{Block, BlockId, ClientId, Clock};
use crate::Document;
use std::ops::Range;

#[derive(Eq, PartialEq, Debug)]
pub struct UpdateBlock<T: Clone> {
    pub(crate) origin_left: Option<BlockId>,
    pub(crate) origin_right: Option<BlockId>,
    pub(crate) value: Option<T>,
}

impl<T: Clone> UpdateBlock<T> {
    fn hydrate(self, id: Clock) -> Block<T> {
        Block {
            id,
            origin_left: self.origin_left,
            origin_right: self.origin_right,
            value: self.value,
            left: None,
            right: None,
            deleted: false,
        }
    }

    fn with_value(left: Option<BlockId>, right: Option<BlockId>, value: T) -> UpdateBlock<T> {
        UpdateBlock {
            origin_left: left,
            origin_right: right,
            value: Some(value),
        }
    }
}

impl<T: Clone> From<Block<T>> for UpdateBlock<T> {
    fn from(block: Block<T>) -> Self {
        UpdateBlock {
            origin_left: block.origin_left,
            origin_right: block.origin_right,
            value: block.value,
        }
    }
}

#[derive(Eq, PartialEq)]
pub struct Update<T: Clone> {
    dependency: Vec<(ClientId, Range<Clock>)>,
    blocks: Vec<(ClientId, Vec<UpdateBlock<T>>)>,
}

#[derive(PartialEq, Debug)]
enum ValidationError {
    ClientDoesNotExist(ClientId),
    UpdateOutsideRange(BlockId),
    InvalidUpdateRange(ClientId),
}

impl<T: Clone> Update<T> {
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
        }
    }

    pub fn apply(self, document: &mut Document<T>) {
        for (client_id, blocks) in self.blocks.into_iter() {
            let hydrated_blocks = blocks
                .into_iter()
                .enumerate()
                .map(|(i, block)| block.hydrate(i as Clock))
                .collect();

            document.store.integrate(client_id, hydrated_blocks)
        }
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
}

#[cfg(test)]
mod tests {
    use crate::document::{Block, BlockId};
    use crate::update::{Update, UpdateBlock, ValidationError};
    use crate::Document;

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
        };

        assert_eq!(valid_update.validate(), Ok(()));
    }

    #[test]
    fn throws_error_if_client_doesnt_exist_range() {
        let valid_update: Update<String> = Update {
            blocks: vec![(1, vec![])],
            dependency: vec![],
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
        };

        assert_eq!(valid_update.validate(), Ok(()));
    }
}
