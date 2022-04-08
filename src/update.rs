use crate::document::{Block, ClientId};
use crate::Document;

#[derive(Eq, PartialEq)]
pub struct Update<T: Clone> {
    blocks: Vec<(ClientId, Vec<Block<T>>)>,
}

impl<T: Clone> Update<T> {
    pub fn from_document(document: &Document<T>) -> Update<T> {
        let blocks = document
            .store
            .data
            .iter()
            .map(|(client_id, block)| (*client_id, block.to_vec()))
            .collect();

        Update { blocks }
    }

    pub fn apply(self, document: &mut Document<T>) {
        for (client_id, blocks) in self.blocks.into_iter() {
            document.store.integrate(client_id, blocks)
        }
    }

    pub(crate) fn from_blocks(client_id: ClientId, blocks: Vec<Block<T>>) -> Update<T> {
        Update {
            blocks: vec![(client_id, blocks)],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::document::Block;
    use crate::update::Update;
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
                vec![Block::with_value(0, None, "test".to_owned())]
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
}
