---
prefix: arch
---

# Ring 1: the raw store

The traits a store adapter implements, below the typed unit of work of
[`ports.req.md`](ports.req.md): text, versions and positions, and nothing of the domain.

## `arch-iv3d` A store adapter shall implement only the raw store, over text, versions and positions

An adapter implements `Store`, `Writer` and `Reader`: one statement per method, over JSON text.
It names no domain type, serializes nothing, numbers no version and checks no read model's
position; `ports` does each of those once, for every store. A rule that each adapter
implements again is a rule one of them gets wrong, and an adapter that handles typed facts has
to be able to build one, which lets everything else build one.

Every method of `Store` takes a `Key` that only `ports` can make, and `Unit` and `View` hold
their writer and reader in a private field, so nothing outside `ports` reaches a raw write: the
shell cannot call `save` or `append`, although the adapter implements them.

```rust
use crate::{Moment, Position, RequestId, StoreError, Version};

pub struct Key(());

impl Key {
    pub(crate) fn new() -> Key {
        Key(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFact<'a> {
    pub position: Position,
    pub stream: String,
    pub version: Version,
    pub event: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRequest {
    pub id: RequestId,
    pub stream: String,
    pub request: String,
    pub attempts: u64,
}

pub type Listener = Box<dyn Fn(Position) + Send + Sync>;

pub trait Reader {
    fn state(&mut self, lifecycle: &'static str, stream: &str) -> Result<Option<(Version, String)>, StoreError>;
    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError>;
    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError>;
    fn refusals(&mut self, lifecycle: &'static str, stream: &str) -> Result<Vec<String>, StoreError>;
}

pub trait Writer: Reader {
    fn append(&mut self, lifecycle: &'static str, stream: &str, version: Version, event: &str) -> Result<Position, StoreError>;
    fn save(&mut self, lifecycle: &'static str, stream: &str, version: Version, state: &str) -> Result<(), StoreError>;
    fn reject(&mut self, lifecycle: &'static str, stream: &str, refusal: &str) -> Result<(), StoreError>;
    fn request(&mut self, lifecycle: &'static str, stream: &str, request: &str) -> Result<(), StoreError>;
    fn finish(&mut self, lifecycle: &'static str, id: RequestId) -> Result<Option<String>, StoreError>;
    fn park(&mut self, lifecycle: &'static str, id: RequestId, cause: &str) -> Result<Option<String>, StoreError>;
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
    fn requests(&self, key: Key, lifecycle: &'static str, limit: usize, now: Moment, until: Moment) -> Result<Vec<StoredRequest>, StoreError>;
    fn mark_failed(&self, key: Key, id: RequestId, again: Moment) -> Result<(), StoreError>;
}
```

A `Writer` method's contract is its name and the conformance suite, which every adapter runs
([adapters](adapters.req.md)).

### Acceptance

- `arch-iv3d.1` The key to the raw store cannot be built or minted outside `ports` (E0603, E0624; mutants C11 and C12).
- `arch-iv3d.2` A `Unit` cannot be built around a writer outside `ports` (E0451; mutant C13).
- `arch-iv3d.3` The `raw-store` recipe fails when a driven adapter depends on `domain` or on a serializer outside its tests (T2, `just check`; mutant G10).
