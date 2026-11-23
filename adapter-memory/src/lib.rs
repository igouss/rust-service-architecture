use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use ports::store::{Key, Listener, Reader, Store, StoredFact, StoredRequest, Writer};
use ports::{Position, RequestId, StoreError, StreamId, Version};

type Stream = (&'static str, String);
type Row = (&'static str, String);

#[derive(Debug, Clone)]
struct Fact {
    lifecycle: &'static str,
    stream: StreamId,
    version: Version,
    event: String,
}

#[derive(Debug, Clone)]
struct Request {
    lifecycle: &'static str,
    stream: StreamId,
    request: String,
    attempts: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parked {
    pub id: RequestId,
    pub lifecycle: &'static str,
    pub stream: StreamId,
    pub request: String,
    pub attempts: u64,
    pub cause: String,
}

#[derive(Debug, Clone)]
struct Rejection {
    lifecycle: &'static str,
    stream: StreamId,
    refusal: String,
}

#[derive(Default)]
struct Tables {
    states: BTreeMap<Stream, (Version, String)>,
    facts: VecDeque<Fact>,
    appended: u64,
    rejections: VecDeque<Rejection>,
    outbox: BTreeMap<u64, Request>,
    parked: Vec<Parked>,
    issued: u64,
    rows: BTreeMap<Row, String>,
    positions: BTreeMap<&'static str, Position>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kept {
    pub facts: usize,
    pub refusals: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub states: usize,
    pub facts: usize,
    pub refusals: usize,
    pub requests: usize,
    pub parked: usize,
    pub rows: usize,
}

#[derive(Default)]
struct Pending {
    states: BTreeMap<Stream, (Version, String)>,
    facts: Vec<Fact>,
    rejections: Vec<Rejection>,
    outbox: Vec<Request>,
    closed: BTreeMap<u64, Option<String>>,
    rows: BTreeMap<Row, Option<String>>,
    positions: BTreeMap<&'static str, Position>,
    cleared: BTreeSet<&'static str>,
}

pub struct Memory {
    tables: RwLock<Tables>,
    kept: Kept,
    listener: Option<Listener>,
}

fn count(length: usize) -> u64 {
    u64::try_from(length).unwrap_or(u64::MAX)
}

fn state_in(tables: &Tables, lifecycle: &'static str, stream: &StreamId) -> Option<(Version, String)> {
    tables.states.get(&(lifecycle, stream.0.clone())).cloned()
}

fn states_in<'a>(tables: &'a Tables, lifecycle: &'static str) -> impl Iterator<Item = (&'a Stream, &'a (Version, String))> {
    tables
        .states
        .range((lifecycle, String::new())..)
        .take_while(move |(stream, _state): &(&Stream, &(Version, String))| stream.0 == lifecycle)
}

fn refusals_in<'a>(rejections: impl Iterator<Item = &'a Rejection>, lifecycle: &'static str, stream: &StreamId) -> Vec<String> {
    rejections
        .filter(|rejection: &&Rejection| rejection.lifecycle == lifecycle && rejection.stream == *stream)
        .map(|rejection: &Rejection| rejection.refusal.clone())
        .collect()
}

fn rows_in<'a>(tables: &'a Tables, read_model: &'static str) -> impl Iterator<Item = (&'a Row, &'a String)> {
    tables
        .rows
        .range((read_model, String::new())..)
        .take_while(move |(row, _text): &(&Row, &String)| row.0 == read_model)
}

impl Memory {
    pub fn new(kept: Kept) -> Memory {
        Memory {
            tables: RwLock::default(),
            kept,
            listener: None,
        }
    }

    pub fn held(&self) -> Held {
        let tables: RwLockReadGuard<'_, Tables> = self.reading();
        Held {
            states: tables.states.len(),
            facts: tables.facts.len(),
            refusals: tables.rejections.len(),
            requests: tables.outbox.len(),
            parked: tables.parked.len(),
            rows: tables.rows.len(),
        }
    }

    pub fn parked(&self) -> Vec<Parked> {
        self.reading().parked.clone()
    }

    pub fn on_commit(self, listener: impl Fn(Position) + Send + Sync + 'static) -> Memory {
        Memory {
            listener: Some(Box::new(listener)),
            ..self
        }
    }

    fn reading(&self) -> RwLockReadGuard<'_, Tables> {
        self.tables.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn writing(&self) -> RwLockWriteGuard<'_, Tables> {
        self.tables.write().unwrap_or_else(PoisonError::into_inner)
    }
}

struct MemoryReader<'a> {
    tables: &'a Tables,
}

impl Reader for MemoryReader<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        Ok(state_in(self.tables, lifecycle, stream))
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        states_in(self.tables, lifecycle).try_for_each(|(stream, state): (&Stream, &(Version, String))| each(StreamId(stream.1.clone()), &state.1))
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self.tables.rows.get(&(read_model, key.to_string())).cloned())
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        rows_in(self.tables, read_model).try_for_each(|(row, text): (&Row, &String)| each(row.1.clone(), text))
    }

    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        Ok(refusals_in(self.tables.rejections.iter(), lifecycle, stream))
    }
}

struct MemoryWriter<'a> {
    tables: &'a Tables,
    pending: Pending,
}

impl MemoryWriter<'_> {
    fn close(&mut self, lifecycle: &'static str, id: RequestId, parked: Option<&str>) -> Option<StreamId> {
        let open: Option<&Request> = self.tables.outbox.get(&id.get()).filter(|request: &&Request| request.lifecycle == lifecycle);
        match open {
            Some(request) if !self.pending.closed.contains_key(&id.get()) => {
                self.pending.closed.insert(id.get(), parked.map(str::to_string));
                Some(request.stream.clone())
            }
            Some(_) | None => None,
        }
    }

    fn merged_states(&self, lifecycle: &'static str) -> BTreeMap<String, String> {
        let mut merged: BTreeMap<String, String> = states_in(self.tables, lifecycle)
            .map(|(stream, state): (&Stream, &(Version, String))| (stream.1.clone(), state.1.clone()))
            .collect();
        merged.extend(
            self.pending
                .states
                .iter()
                .filter(|(stream, _state): &(&Stream, &(Version, String))| stream.0 == lifecycle)
                .map(|(stream, state): (&Stream, &(Version, String))| (stream.1.clone(), state.1.clone())),
        );
        merged
    }

    fn merged_rows(&self, read_model: &'static str) -> BTreeMap<String, String> {
        let mut merged: BTreeMap<String, String> = match self.pending.cleared.contains(read_model) {
            true => BTreeMap::new(),
            false => rows_in(self.tables, read_model)
                .map(|(row, text): (&Row, &String)| (row.1.clone(), text.clone()))
                .collect(),
        };
        for (row, text) in self.pending.rows.iter().filter(|(row, _text): &(&Row, &Option<String>)| row.0 == read_model) {
            match text {
                Some(text) => merged.insert(row.1.clone(), text.clone()),
                None => merged.remove(&row.1),
            };
        }
        merged
    }
}

impl Reader for MemoryWriter<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        match self.pending.states.get(&(lifecycle, stream.0.clone())) {
            Some(state) => Ok(Some(state.clone())),
            None => Ok(state_in(self.tables, lifecycle, stream)),
        }
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.merged_states(lifecycle)
            .into_iter()
            .try_for_each(|(stream, state): (String, String)| each(StreamId(stream), &state))
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        let row: Row = (read_model, key.to_string());
        match (self.pending.rows.get(&row), self.pending.cleared.contains(read_model)) {
            (Some(text), _) => Ok(text.clone()),
            (None, true) => Ok(None),
            (None, false) => Ok(self.tables.rows.get(&row).cloned()),
        }
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.merged_rows(read_model)
            .into_iter()
            .try_for_each(|(key, text): (String, String)| each(key, &text))
    }

    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        Ok(refusals_in(
            self.tables.rejections.iter().chain(self.pending.rejections.iter()),
            lifecycle,
            stream,
        ))
    }
}

impl Writer for MemoryWriter<'_> {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError> {
        self.pending.facts.push(Fact {
            lifecycle,
            stream: stream.clone(),
            version,
            event: event.to_string(),
        });
        Ok(Position::ZERO.after(self.tables.appended + count(self.pending.facts.len())))
    }

    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError> {
        self.pending.states.insert((lifecycle, stream.0.clone()), (version, state.to_string()));
        Ok(())
    }

    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError> {
        self.pending.rejections.push(Rejection {
            lifecycle,
            stream: stream.clone(),
            refusal: refusal.to_string(),
        });
        Ok(())
    }

    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError> {
        self.pending.outbox.push(Request {
            lifecycle,
            stream: stream.clone(),
            request: request.to_string(),
            attempts: 0,
        });
        Ok(())
    }

    fn finish(&mut self, lifecycle: &'static str, id: RequestId) -> Result<Option<StreamId>, StoreError> {
        Ok(self.close(lifecycle, id, None))
    }

    fn park(&mut self, lifecycle: &'static str, id: RequestId, cause: &str) -> Result<Option<StreamId>, StoreError> {
        Ok(self.close(lifecycle, id, Some(cause)))
    }

    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let trimmed: u64 = self.tables.appended - count(self.tables.facts.len());
        if after.get() < trimmed {
            return Err(StoreError::Trimmed {
                after,
                oldest: Position::ZERO.after(trimmed + 1),
            });
        }
        self.tables
            .facts
            .iter()
            .chain(self.pending.facts.iter())
            .enumerate()
            .skip(usize::try_from(after.get() - trimmed).unwrap_or(usize::MAX))
            .filter(|(_at, fact): &(usize, &Fact)| fact.lifecycle == lifecycle)
            .take(limit)
            .try_for_each(|(at, fact): (usize, &Fact)| {
                each(StoredFact {
                    position: Position::ZERO.after(trimmed + count(at + 1)),
                    stream: fact.stream.clone(),
                    version: fact.version,
                    event: &fact.event,
                })
            })
    }

    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError> {
        match (self.pending.positions.get(read_model), self.pending.cleared.contains(read_model)) {
            (Some(position), _) => Ok(*position),
            (None, true) => Ok(Position::ZERO),
            (None, false) => Ok(self.tables.positions.get(read_model).copied().unwrap_or(Position::ZERO)),
        }
    }

    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError> {
        self.pending.positions.insert(read_model, position);
        Ok(())
    }

    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError> {
        self.pending.rows.insert((read_model, key.to_string()), Some(row.to_string()));
        Ok(())
    }

    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError> {
        self.pending.rows.insert((read_model, key.to_string()), None);
        Ok(())
    }

    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError> {
        self.pending.rows.retain(|row: &Row, _text: &mut Option<String>| row.0 != read_model);
        self.pending.positions.remove(read_model);
        self.pending.cleared.insert(read_model);
        Ok(())
    }
}

fn trim<T>(queue: &mut VecDeque<T>, kept: usize) {
    let excess: usize = queue.len().saturating_sub(kept);
    queue.drain(..excess);
}

fn commit(tables: &mut Tables, pending: Pending, kept: Kept) {
    for read_model in &pending.cleared {
        tables.rows.retain(|row: &Row, _text: &mut String| row.0 != *read_model);
        tables.positions.remove(read_model);
    }
    tables.states.extend(pending.states);
    tables.appended += count(pending.facts.len());
    tables.facts.extend(pending.facts);
    tables.rejections.extend(pending.rejections);
    for (id, cause) in pending.closed {
        let closed: Option<Request> = tables.outbox.remove(&id);
        if let (Some(request), Some(cause)) = (closed, cause) {
            tables.parked.push(Parked {
                id: RequestId::ZERO.after(id),
                lifecycle: request.lifecycle,
                stream: request.stream,
                request: request.request,
                attempts: request.attempts + 1,
                cause,
            });
        }
    }
    for request in pending.outbox {
        tables.issued += 1;
        tables.outbox.insert(tables.issued, request);
    }
    tables.positions.extend(pending.positions);
    for (row, text) in pending.rows {
        match text {
            Some(text) => tables.rows.insert(row, text),
            None => tables.rows.remove(&row),
        };
    }
    trim(&mut tables.facts, kept.facts);
    trim(&mut tables.rejections, kept.refusals);
}

impl Store for Memory {
    fn transact<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Writer) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let mut tables: RwLockWriteGuard<'_, Tables> = self.writing();
        let mut writer: MemoryWriter<'_> = MemoryWriter {
            tables: &tables,
            pending: Pending::default(),
        };
        let value: T = work(&mut writer)?;
        let pending: Pending = writer.pending;
        commit(&mut tables, pending, self.kept);
        Ok(value)
    }

    fn snapshot<T>(&self, _key: Key, work: impl FnOnce(&mut dyn Reader) -> Result<T, StoreError>) -> Result<T, StoreError> {
        let tables: RwLockReadGuard<'_, Tables> = self.reading();
        work(&mut MemoryReader { tables: &tables })
    }

    fn committed(&self, _key: Key, last: Position) {
        if let Some(listener) = &self.listener {
            listener(last);
        }
    }

    fn requests(&self, _key: Key, lifecycle: &'static str, limit: usize) -> Result<Vec<StoredRequest>, StoreError> {
        let tables: RwLockReadGuard<'_, Tables> = self.reading();
        let mut open: Vec<(u64, u64, &Request)> = tables
            .outbox
            .iter()
            .filter(|(_id, request): &(&u64, &Request)| request.lifecycle == lifecycle)
            .map(|(id, request): (&u64, &Request)| (request.attempts, *id, request))
            .collect();
        open.sort_by_key(|(attempts, id, _request): &(u64, u64, &Request)| (*attempts, *id));
        Ok(open
            .into_iter()
            .take(limit)
            .map(|(attempts, id, request): (u64, u64, &Request)| StoredRequest {
                id: RequestId::ZERO.after(id),
                stream: request.stream.clone(),
                request: request.request.clone(),
                attempts,
            })
            .collect())
    }

    fn mark_failed(&self, _key: Key, id: RequestId) -> Result<(), StoreError> {
        if let Some(request) = self.writing().outbox.get_mut(&id.get()) {
            request.attempts += 1;
        }
        Ok(())
    }
}
