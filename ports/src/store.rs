use crate::{Position, RequestId, StoreError, StreamId, Version};

pub struct Key(());

impl Key {
    pub(crate) fn new() -> Key {
        Key(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFact<'a> {
    pub position: Position,
    pub stream: StreamId,
    pub version: Version,
    pub event: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRequest {
    pub id: RequestId,
    pub stream: StreamId,
    pub request: String,
}

pub type Listener = Box<dyn Fn(Position) + Send + Sync>;

pub trait Reader {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError>;
    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError>;
    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError>;
}

pub trait Writer: Reader {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError>;
    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError>;
    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError>;
    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError>;
    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError>;
    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError>;
    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError>;
    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError>;
    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError>;
    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError>;
}

pub trait Store {
    fn transact<T>(&self, key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn snapshot<T>(&self, key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError>;
    fn committed(&self, key: Key, last: Position);
    fn requests(&self, key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError>;
    fn mark_done(&self, key: Key, id: RequestId) -> Result<(), StoreError>;
    fn mark_failed(&self, key: Key, id: RequestId) -> Result<(), StoreError>;
}
