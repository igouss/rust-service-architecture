use domain::{Command, Facts, Lifecycle, Never, Projection, Reaction, Step};
use ports::{Applied, Claimed, Finished, Journal, Moment, Outbox, Position, Recorded, RequestId, StoreError, StreamId, Version, View};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Counter {
    Empty,
    Holding { total: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Counted {
    Added { by: u64 },
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum CounterRefusal {
    NothingToAdd,
}

impl Lifecycle for Counter {
    const NAME: &'static str = "suite_counter";
    type Event = Counted;
    type Refusal = CounterRefusal;

    fn initial() -> Counter {
        Counter::Empty
    }

    fn evolve(self, event: &Counted) -> Counter {
        match (self, event) {
            (Counter::Empty, Counted::Added { by }) => Counter::Holding { total: *by },
            (Counter::Holding { total }, Counted::Added { by }) => Counter::Holding { total: total + by },
            (Counter::Empty | Counter::Holding { .. }, Counted::Reset) => Counter::Empty,
        }
    }
}

struct Add {
    by: u64,
}

impl Command for Add {
    type Reads = Counter;

    fn decide(self, _reads: &Counter) -> Result<Facts<Counter>, CounterRefusal> {
        match self.by {
            0 => Err(CounterRefusal::NothingToAdd),
            by => Ok(vec![Counted::Added { by }]),
        }
    }
}

struct AddTwice {
    by: u64,
}

impl Command for AddTwice {
    type Reads = Counter;

    fn decide(self, _reads: &Counter) -> Result<Facts<Counter>, CounterRefusal> {
        Ok(vec![Counted::Added { by: self.by }, Counted::Added { by: self.by }])
    }
}

struct Reset;

impl Command for Reset {
    type Reads = Counter;

    fn decide(self, _reads: &Counter) -> Result<Facts<Counter>, CounterRefusal> {
        Ok(vec![Counted::Reset])
    }
}

struct Total;

impl Projection for Total {
    const NAME: &'static str = "suite_total_v1";
    type Reads = Counter;
    type Row = u64;

    fn key(_stream: &str, _event: &Counted) -> Option<String> {
        Some("all".to_string())
    }

    fn apply(row: Option<u64>, event: &Counted) -> Option<u64> {
        match event {
            Counted::Added { by } => Some(row.unwrap_or(0) + by),
            Counted::Reset => row,
        }
    }
}

struct PerStream;

impl Projection for PerStream {
    const NAME: &'static str = "suite_per_stream_v1";
    type Reads = Counter;
    type Row = u64;

    fn key(stream: &str, _event: &Counted) -> Option<String> {
        Some(stream.to_string())
    }

    fn apply(row: Option<u64>, event: &Counted) -> Option<u64> {
        match event {
            Counted::Added { by } => Some(row.unwrap_or(0) + by),
            Counted::Reset => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Courier {
    Idle,
    Sent { parcels: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Dispatched {
    Parcels { count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Parcel {
    number: u64,
}

impl Lifecycle for Courier {
    const NAME: &'static str = "suite_courier";
    type Event = Dispatched;
    type Refusal = Never;

    fn initial() -> Courier {
        Courier::Idle
    }

    fn evolve(self, event: &Dispatched) -> Courier {
        match (self, event) {
            (Courier::Idle, Dispatched::Parcels { count }) => Courier::Sent { parcels: *count },
            (Courier::Sent { parcels }, Dispatched::Parcels { count }) => Courier::Sent { parcels: parcels + count },
        }
    }
}

struct Send {
    number: u64,
}

impl Reaction for Send {
    type Reads = Courier;
    type Request = Parcel;

    fn react(self, _reads: &Courier) -> Step<Dispatched, Parcel> {
        Step {
            events: vec![Dispatched::Parcels { count: 1 }],
            requests: vec![Parcel { number: self.number }],
        }
    }
}

struct Receive {
    count: u64,
}

impl Reaction for Receive {
    type Reads = Courier;
    type Request = Parcel;

    fn react(self, _reads: &Courier) -> Step<Dispatched, Parcel> {
        Step {
            events: vec![Dispatched::Parcels { count: self.count }],
            requests: Vec::new(),
        }
    }
}

struct Remind {
    number: u64,
}

impl Reaction for Remind {
    type Reads = Courier;
    type Request = Parcel;

    fn react(self, _reads: &Courier) -> Step<Dispatched, Parcel> {
        Step {
            events: Vec::new(),
            requests: vec![Parcel { number: self.number }],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Porter {
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Carried {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Bag {
    tag: u64,
}

impl Lifecycle for Porter {
    const NAME: &'static str = "suite_porter";
    type Event = Carried;
    type Refusal = Never;

    fn initial() -> Porter {
        Porter::Idle
    }

    fn evolve(self, event: &Carried) -> Porter {
        match *event {}
    }
}

struct Carry {
    tag: u64,
}

impl Reaction for Carry {
    type Reads = Porter;
    type Request = Bag;

    fn react(self, _reads: &Porter) -> Step<Carried, Bag> {
        Step {
            events: Vec::new(),
            requests: vec![Bag { tag: self.tag }],
        }
    }
}

type Seen = (Position, StreamId<Counter>, Version, Counted);

fn id<S>(text: &str) -> StreamId<S> {
    StreamId::new(text)
}

fn seen(facts: &[Recorded<Counter>]) -> Vec<Seen> {
    facts
        .iter()
        .map(|fact: &Recorded<Counter>| (fact.position(), fact.stream().clone(), fact.version(), fact.event().clone()))
        .collect()
}

fn added(position: u64, stream: &str, version: u64, by: u64) -> Seen {
    (Position::ZERO.after(position), id(stream), Version::ZERO.after(version), Counted::Added { by })
}

fn add(store: &Subject, stream: &str, by: u64) -> Vec<Recorded<Counter>> {
    let applied: Applied<Counter> = store
        .write(|unit| unit.apply(&id(stream), |state: Counter| ports::decide(Add { by }, state)))
        .expect("applied");
    applied.facts
}

fn first_fact(store: &Subject, stream: &str, by: u64) -> Recorded<Counter> {
    add(store, stream, by).into_iter().next().expect("one fact")
}

fn counter(store: &Subject, stream: &str) -> (Version, Counter) {
    store.read(|view| view.state::<Counter>(&id(stream))).expect("read")
}

fn counter_facts(store: &Subject, after: u64, limit: usize) -> Vec<Seen> {
    seen(&store.write(|unit| unit.facts::<Counter>(Position::ZERO.after(after), limit)).expect("facts"))
}

fn send(store: &Subject, stream: &str, number: u64) {
    let _applied: Applied<Courier> = store
        .write(|unit| unit.apply(&id(stream), |state: Courier| ports::react(Send { number }, state)))
        .expect("applied");
}

fn facts_of<S: Lifecycle>(finished: Finished<S>) -> Option<usize> {
    match finished {
        Finished::Applied(applied) => Some(applied.facts.len()),
        Finished::Already => None,
    }
}

fn receive(store: &Subject, request: RequestId, count: u64) -> Option<usize> {
    facts_of(
        store
            .write(|unit| unit.finish(request, |state: Courier| ports::react(Receive { count }, state)))
            .expect("finished"),
    )
}

fn shelve(store: &Subject, request: RequestId, count: u64) -> Option<usize> {
    facts_of(
        store
            .write(|unit| unit.park(request, "refused", |state: Courier| ports::react(Receive { count }, state)))
            .expect("parked"),
    )
}

fn attempts(store: &Subject) -> Vec<u64> {
    store
        .claim::<Send>(10, Moment::ZERO, Moment::ZERO)
        .expect("claimed")
        .into_iter()
        .map(|claimed: Claimed<Parcel>| claimed.attempts)
        .collect()
}

fn courier(store: &Subject, stream: &str) -> (Version, Courier) {
    store.read(|view| view.state::<Courier>(&id(stream))).expect("read")
}

fn parcels(store: &Subject, limit: usize) -> Vec<(StreamId<Courier>, Parcel)> {
    store
        .claim::<Send>(limit, Moment::ZERO, Moment::ZERO)
        .expect("claimed")
        .into_iter()
        .map(|claimed: Claimed<Parcel>| (StreamId::new(claimed.stream), claimed.request))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Atlas;

type Charted = std::collections::BTreeMap<(u8, u8), u8>;

impl Lifecycle for Atlas {
    const NAME: &'static str = "suite_atlas";
    type Event = Charted;
    type Refusal = Never;

    fn initial() -> Atlas {
        Atlas
    }

    fn evolve(self, _event: &Charted) -> Atlas {
        Atlas
    }
}

struct Chart;

impl Command for Chart {
    type Reads = Atlas;

    fn decide(self, _reads: &Atlas) -> Result<Facts<Atlas>, Never> {
        Ok(vec![Charted::from([((0, 0), 0)])])
    }
}
