use crate::document::{Block, BlockId, ClientId, Clock};
use std::collections::HashMap;
use std::ops::{Index, IndexMut};

#[derive(Debug)]
pub struct Store<T: Clone> {
    start: Option<BlockId>,
    end: Option<BlockId>,
    client_id: u64,
    pub(crate) data: HashMap<ClientId, Vec<Block<T>>>,
}

pub struct BlockWithClientId<'a, T: Clone> {
    block: &'a Block<T>,
    block_id: BlockId,
}

impl<T: Clone> Store<T> {
    pub(crate) fn integrate(&mut self, client_id: ClientId, mut blocks: Vec<Block<T>>) {
        for block in blocks.iter_mut() {
            let insert_before =
                self.find_insertion_point(client_id, block.origin_left, block.origin_right);

            block.right = insert_before;

            let new_block_id = Some(BlockId {
                client_id,
                clock: block.id,
            });

            if let Some(insert_before) = insert_before {
                let to_right = &mut self[insert_before];

                block.left = to_right.left;

                let previous_left = to_right.left;
                to_right.left = Some(BlockId {
                    client_id,
                    clock: block.id,
                });

                if let Some(to_left) = previous_left {
                    // insert in between `to_left` and `to_right`
                    let to_left = &mut self[to_left];

                    to_left.right = new_block_id;
                } else {
                    // insert at start

                    self.start = new_block_id;
                }
            } else if let Some(end) = self.end {
                // insert at end
                let end = &mut self[end];
                end.right = new_block_id;
                self.end = new_block_id;
            } else {
                // is empty

                self.start = new_block_id;
                self.end = new_block_id;
            }
        }

        self.data.insert(client_id, blocks);
    }

    fn find_insertion_point(
        &self,
        client_id: ClientId,
        left: Option<BlockId>,
        right: Option<BlockId>,
    ) -> Option<BlockId> {
        for BlockWithClientId { block_id, block } in self.iter_blocks_with_offset(left) {
            if Some(block_id) == right {
                // We've reached the end of insertion search, insert at this point

                return Some(block_id);
            }

            if block.left == left && block_id.client_id > client_id {
                // This block conflicts, but has a larger client id

                return Some(block_id);
            }
        }

        None
    }
}

// Insertion point is found if:
// Right satisfies:
//    Left = My left

impl<T: Clone> Index<BlockId> for Store<T> {
    type Output = Block<T>;

    fn index(&self, BlockId { client_id, clock }: BlockId) -> &Self::Output {
        &self.data[&client_id][clock as usize]
    }
}

impl<T: Clone> IndexMut<BlockId> for Store<T> {
    fn index_mut(&mut self, BlockId { client_id, clock }: BlockId) -> &mut Self::Output {
        &mut self.data.get_mut(&client_id).unwrap()[clock as usize]
    }
}

impl<T: Clone> Store<T> {
    pub fn new(client_id: u64) -> Store<T> {
        Store {
            data: HashMap::new(),
            start: None,
            end: None,
            client_id,
        }
    }

    pub fn append(&mut self, value: T) {
        self.add_block(self.end, None, value);
    }

    pub fn insert(&mut self, index: usize, value: T) {
        let (previous, next) = if let Some(previous) = self
            .iter_blocks()
            .filter(|b| !b.block.deleted)
            .take(index)
            .next()
        {
            // at the start
            (Some(previous.block_id), previous.block.right)
        } else {
            // at the end
            (None, None)
        };

        self.add_block(previous, next, value);
    }

    fn add_block(&mut self, previous: Option<BlockId>, next: Option<BlockId>, value: T) {
        let block_id = if let Some(v) = self.data.get_mut(&self.client_id) {
            let id = v.len() as u64;

            let block = Block::with_value_and_right(id, previous, next, value);

            v.push(block);

            BlockId {
                client_id: self.client_id,
                clock: id,
            }
        } else {
            let block = Block::with_value_and_right(0, previous, next, value);

            self.data.insert(self.client_id, vec![block]);

            BlockId {
                client_id: self.client_id,
                clock: 0,
            }
        };

        if let Some(next) = next {
            let next_block = &mut self[next];

            next_block.left = Some(block_id);
        } else {
            if let Some(end) = self.end {
                let end_block = &mut self[end];
                end_block.right = Some(block_id);
            }

            self.end = Some(block_id);
        }

        if let Some(previous) = previous {
            let prev_block = &mut self[previous];

            prev_block.right = Some(block_id);
        } else {
            if let Some(start) = self.start {
                let start_block = &mut self[start];
                start_block.left = Some(block_id);
            }

            self.start = Some(block_id);
        }
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = BlockWithClientId<T>> {
        StoreIterator {
            store: self,
            current: None,
            is_done: false,
        }
    }

    pub fn iter_blocks_with_offset(
        &self,
        start: Option<BlockId>,
    ) -> impl Iterator<Item = BlockWithClientId<T>> {
        StoreIterator {
            store: self,
            current: start,
            is_done: false,
        }
    }

    pub fn iter_values(&self) -> impl Iterator<Item = &T> {
        self.iter_blocks()
            .flat_map(|BlockWithClientId { block, .. }| &block.value)
    }
}

struct StoreIterator<'a, T: Clone> {
    store: &'a Store<T>,
    current: Option<BlockId>,
    is_done: bool,
}

impl<'a, T: Clone> Iterator for StoreIterator<'a, T> {
    type Item = BlockWithClientId<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }

        if self.current.is_none() {
            self.current = self.store.start;
        }

        self.current
            .map(|current| BlockWithClientId {
                block_id: current,
                block: &self.store[current],
            })
            .map(|block| {
                self.current = block.block.right;

                if self.current.is_none() {
                    self.is_done = true;
                }

                block
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::document::{Block, BlockId};
    use crate::store::Store;

    #[test]
    fn insert_at_start_when_empty() {
        let store: Store<String> = Store::new(1);

        let insertion_point = store.find_insertion_point(2, None, None);

        assert_eq!(insertion_point, None);
    }

    #[test]
    fn insert_at_start_before_existing() {
        let mut store: Store<String> = Store::new(3);
        store.append("Test".to_owned());

        let insertion_point = store.find_insertion_point(2, None, None);

        assert_eq!(insertion_point, Some(BlockId::new(3, 0)))
    }

    #[test]
    fn insert_at_start_after_existing() {
        let mut store: Store<String> = Store::new(1);
        store.append("Test".to_owned());

        let insertion_point = store.find_insertion_point(2, None, None);

        assert_eq!(insertion_point, None)
    }

    #[test]
    fn insert_no_conflicts() {
        let mut store: Store<String> = Store::new(1);
        store.append("Test".to_owned());
        store.append("Test 2".to_owned());

        let insertion_point =
            store.find_insertion_point(2, Some(BlockId::new(1, 0)), Some(BlockId::new(1, 1)));

        assert_eq!(insertion_point, Some(BlockId::new(1, 1)))
    }

    #[test]
    fn integrate_changes() {
        let mut store: Store<String> = Store::new(1);
        store.append("Test".to_owned());
        store.append("Test 2".to_owned());

        store.integrate(
            2,
            vec![Block::with_value_and_right(
                0,
                Some(BlockId::new(1, 0)),
                Some(BlockId::new(1, 1)),
                "Test 3".to_owned(),
            )],
        );

        assert_eq!(
            store.iter_values().collect::<Vec<&String>>(),
            vec!["Test", "Test 3", "Test 2"]
        )
    }

    #[test]
    fn insert_changes() {
        let mut store: Store<String> = Store::new(1);
        store.append("Test".to_owned());
        store.append("Test 2".to_owned());
        store.insert(1, "Test 3".to_owned());

        assert_eq!(
            store.iter_values().collect::<Vec<&String>>(),
            vec!["Test", "Test 3", "Test 2"]
        )
    }

    #[test]
    fn integrate_conflicts() {
        let mut store: Store<String> = Store::new(1);
        store.append("Test".to_owned());
        store.append("Test 2".to_owned());
        store.insert(1, "Test 3".to_owned());

        store.integrate(
            2,
            vec![Block::with_value_and_right(
                0,
                Some(BlockId::new(1, 0)),
                Some(BlockId::new(1, 1)),
                "Test 4".to_owned(),
            )],
        );

        assert_eq!(
            store.iter_values().collect::<Vec<&String>>(),
            vec!["Test", "Test 3", "Test 4", "Test 2"]
        )
    }

    #[test]
    fn integrate_conflicts_smaller_client_id() {
        let mut store: Store<String> = Store::new(2);
        store.append("Test".to_owned());
        store.append("Test 2".to_owned());
        store.insert(1, "Test 3".to_owned());

        store.integrate(
            1,
            vec![Block::with_value_and_right(
                0,
                Some(BlockId::new(2, 0)),
                Some(BlockId::new(2, 1)),
                "Test 4".to_owned(),
            )],
        );

        assert_eq!(
            store.iter_values().collect::<Vec<&String>>(),
            vec!["Test", "Test 4", "Test 3", "Test 2"]
        )
    }
}
