use ports::store::{Reader, StoredFact, Writer};
use ports::{Position, RequestId, StoreError, StreamId, Version};
use postgres::{Client, Row};

use crate::statements::Statements;
use crate::{number, text, Session};

impl Session<'_> {
    fn close(&self, lifecycle: &'static str, id: RequestId, failures: i64) -> Result<Option<StreamId>, StoreError> {
        let id: i64 = id.stored();
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.close, &[&id, &lifecycle, &failures]))?
        {
            None => Ok(None),
            Some(row) => Ok(Some(StreamId(text("outbox", &row, 0)?.to_string()))),
        }
    }
}

impl Reader for Session<'_> {
    fn state(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Option<(Version, String)>, StoreError> {
        let stream: String = stream.0.clone();
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.state, &[&lifecycle, &stream]))?
        {
            None => Ok(None),
            Some(row) => Ok(Some((number("states", &row, 0, Version::loaded)?, text("states", &row, 1)?.to_string()))),
        }
    }

    fn states(&mut self, lifecycle: &'static str, each: &mut dyn FnMut(StreamId, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.states, &[&lifecycle]))?
            .iter()
            .try_for_each(|row: &Row| each(StreamId(text("states", row, 0)?.to_string()), text("states", row, 1)?))
    }

    fn row(&mut self, read_model: &'static str, key: &str) -> Result<Option<String>, StoreError> {
        let key: String = key.to_string();
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.row, &[&read_model, &key]))?
        {
            None => Ok(None),
            Some(row) => Ok(Some(text("read_models", &row, 0)?.to_string())),
        }
    }

    fn rows(&mut self, read_model: &'static str, each: &mut dyn FnMut(String, &str) -> Result<(), StoreError>) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.rows, &[&read_model]))?
            .iter()
            .try_for_each(|row: &Row| each(text("read_models", row, 0)?.to_string(), text("read_models", row, 1)?))
    }

    fn refusals(&mut self, lifecycle: &'static str, stream: &StreamId) -> Result<Vec<String>, StoreError> {
        let stream: String = stream.0.clone();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.rejections, &[&lifecycle, &stream]))?
            .iter()
            .map(|row: &Row| text("rejections", row, 0).map(str::to_string))
            .collect()
    }
}

impl Writer for Session<'_> {
    fn append(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, event: &str) -> Result<Position, StoreError> {
        let (stream, version, event): (String, i64, String) = (stream.0.clone(), version.stored(), event.to_string());
        let row: Row = self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_one(&statements.append, &[&lifecycle, &stream, &version, &event]))?;
        number("facts", &row, 0, Position::loaded)
    }

    fn save(&mut self, lifecycle: &'static str, stream: &StreamId, version: Version, state: &str) -> Result<(), StoreError> {
        let (stream, version, state): (String, i64, String) = (stream.0.clone(), version.stored(), state.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.save, &[&lifecycle, &stream, &version, &state]))
            .map(|_changed: u64| ())
    }

    fn reject(&mut self, lifecycle: &'static str, stream: &StreamId, refusal: &str) -> Result<(), StoreError> {
        let (stream, refusal): (String, String) = (stream.0.clone(), refusal.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.reject, &[&lifecycle, &stream, &refusal]))
            .map(|_changed: u64| ())
    }

    fn request(&mut self, lifecycle: &'static str, stream: &StreamId, request: &str) -> Result<(), StoreError> {
        let (stream, request): (String, String) = (stream.0.clone(), request.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.request, &[&lifecycle, &stream, &request]))
            .map(|_changed: u64| ())
    }

    fn finish(&mut self, lifecycle: &'static str, id: RequestId) -> Result<Option<StreamId>, StoreError> {
        self.close(lifecycle, id, 0)
    }

    fn park(&mut self, lifecycle: &'static str, id: RequestId, cause: &str) -> Result<Option<StreamId>, StoreError> {
        let parked: Option<StreamId> = self.close(lifecycle, id, 1)?;
        if parked.is_some() {
            let (id, cause): (i64, String) = (id.stored(), cause.to_string());
            self.worker
                .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.park, &[&id, &cause]))
                .map(|_changed: u64| ())?;
        }
        Ok(parked)
    }

    fn facts(
        &mut self,
        lifecycle: &'static str,
        after: Position,
        limit: usize,
        each: &mut dyn FnMut(StoredFact<'_>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let (after, limit): (i64, i64) = (after.stored(), i64::try_from(limit).unwrap_or(i64::MAX));
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.query(&statements.facts, &[&lifecycle, &after, &limit]))?
            .iter()
            .try_for_each(|row: &Row| {
                each(StoredFact {
                    position: number("facts", row, 0, Position::loaded)?,
                    stream: StreamId(text("facts", row, 1)?.to_string()),
                    version: number("facts", row, 2, Version::loaded)?,
                    event: text("facts", row, 3)?,
                })
            })
    }

    fn position(&mut self, read_model: &'static str) -> Result<Position, StoreError> {
        match self
            .worker
            .call(move |client: &mut Client, statements: &Statements| client.query_opt(&statements.position, &[&read_model]))?
        {
            None => Ok(Position::ZERO),
            Some(row) => Ok(number("read_model_positions", &row, 0, Position::loaded)?),
        }
    }

    fn advance(&mut self, read_model: &'static str, position: Position) -> Result<(), StoreError> {
        let position: i64 = position.stored();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.advance, &[&read_model, &position]))
            .map(|_changed: u64| ())
    }

    fn upsert(&mut self, read_model: &'static str, key: &str, row: &str) -> Result<(), StoreError> {
        let (key, row): (String, String) = (key.to_string(), row.to_string());
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.upsert, &[&read_model, &key, &row]))
            .map(|_changed: u64| ())
    }

    fn delete(&mut self, read_model: &'static str, key: &str) -> Result<(), StoreError> {
        let key: String = key.to_string();
        self.worker
            .call(move |client: &mut Client, statements: &Statements| client.execute(&statements.delete, &[&read_model, &key]))
            .map(|_changed: u64| ())
    }

    fn clear(&mut self, read_model: &'static str) -> Result<(), StoreError> {
        self.worker
            .call(move |client: &mut Client, statements: &Statements| {
                client.execute(&statements.clear_rows, &[&read_model])?;
                client.execute(&statements.clear_position, &[&read_model])
            })
            .map(|_changed: u64| ())
    }
}
