#![allow(clippy::expect_used, clippy::panic)]

use adapter_sqlite::Sqlite;
use app::{advance, react, Policy};
use domain::{Lifecycle, Never, Reaction, Step};
use ports::{Journal, Recorded, StoreError, StreamId, Unit, Version, View};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Ping {
    Still,
    Struck { times: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Pong {
    Still,
    Struck { times: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Rally {
    Struck,
}

impl Lifecycle for Ping {
    const NAME: &'static str = "cascade_ping";
    type Event = Rally;
    type Refusal = Never;

    fn initial() -> Ping {
        Ping::Still
    }

    fn evolve(self, event: &Rally) -> Ping {
        match (self, event) {
            (Ping::Still, Rally::Struck) => Ping::Struck { times: 1 },
            (Ping::Struck { times }, Rally::Struck) => Ping::Struck { times: times + 1 },
        }
    }
}

impl Lifecycle for Pong {
    const NAME: &'static str = "cascade_pong";
    type Event = Rally;
    type Refusal = Never;

    fn initial() -> Pong {
        Pong::Still
    }

    fn evolve(self, event: &Rally) -> Pong {
        match (self, event) {
            (Pong::Still, Rally::Struck) => Pong::Struck { times: 1 },
            (Pong::Struck { times }, Rally::Struck) => Pong::Struck { times: times + 1 },
        }
    }
}

struct Serve;

impl Reaction for Serve {
    type Reads = Ping;
    type Request = Never;

    fn react(self, _reads: &Ping) -> Step<Rally, Never> {
        Step {
            events: vec![Rally::Struck],
            requests: Vec::new(),
        }
    }
}

struct Return;

impl Reaction for Return {
    type Reads = Pong;
    type Request = Never;

    fn react(self, _reads: &Pong) -> Step<Rally, Never> {
        Step {
            events: vec![Rally::Struck],
            requests: Vec::new(),
        }
    }
}

impl Policy for Ping {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Ping>) -> Result<(), StoreError> {
        react(unit, &StreamId::<Pong>::new(fact.stream().as_str()), Return)
    }
}

impl Policy for Pong {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Pong>) -> Result<(), StoreError> {
        react(unit, &StreamId::<Ping>::new(fact.stream().as_str()), Serve)
    }
}

fn ping(store: &Sqlite) -> (Version, Ping) {
    store.read(|view: &mut View<'_>| view.state::<Ping>(&StreamId::new("r-1"))).expect("read")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Chain {
    Loose,
    Linked { links: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Forged {
    Link { left: u64 },
}

impl Lifecycle for Chain {
    const NAME: &'static str = "cascade_chain";
    type Event = Forged;
    type Refusal = Never;

    fn initial() -> Chain {
        Chain::Loose
    }

    fn evolve(self, event: &Forged) -> Chain {
        match (self, event) {
            (Chain::Loose, Forged::Link { .. }) => Chain::Linked { links: 1 },
            (Chain::Linked { links }, Forged::Link { .. }) => Chain::Linked { links: links + 1 },
        }
    }
}

struct Forge {
    left: u64,
}

impl Reaction for Forge {
    type Reads = Chain;
    type Request = Never;

    fn react(self, _reads: &Chain) -> Step<Forged, Never> {
        Step {
            events: vec![Forged::Link { left: self.left }],
            requests: Vec::new(),
        }
    }
}

impl Policy for Chain {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Chain>) -> Result<(), StoreError> {
        match fact.event() {
            Forged::Link { left: 0 } => Ok(()),
            Forged::Link { left } => react(unit, fact.stream(), Forge { left: left - 1 }),
        }
    }
}

fn chain(store: &Sqlite) -> (Version, Chain) {
    store.read(|view: &mut View<'_>| view.state::<Chain>(&StreamId::new("c-1"))).expect("read")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Walker {
    Resting,
    Walked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Paced {
    Step { left: u64, spread: u64 },
}

impl Lifecycle for Walker {
    const NAME: &'static str = "cascade_walker";
    type Event = Paced;
    type Refusal = Never;

    fn initial() -> Walker {
        Walker::Resting
    }

    fn evolve(self, event: &Paced) -> Walker {
        match (self, event) {
            (Walker::Resting | Walker::Walked, Paced::Step { .. }) => Walker::Walked,
        }
    }
}

struct Walk {
    left: u64,
    spread: u64,
}

impl Reaction for Walk {
    type Reads = Walker;
    type Request = Never;

    fn react(self, _reads: &Walker) -> Step<Paced, Never> {
        Step {
            events: vec![Paced::Step {
                left: self.left,
                spread: self.spread,
            }],
            requests: Vec::new(),
        }
    }
}

impl Policy for Walker {
    fn after(unit: &mut Unit<'_>, fact: &Recorded<Walker>) -> Result<(), StoreError> {
        let Paced::Step { left, spread } = fact.event();
        (0..*spread).try_for_each(|spoke: u64| react(unit, &StreamId::<Walker>::new(format!("spoke-{spoke}")), Walk { left: 0, spread: 0 }))?;
        match left {
            0 => Ok(()),
            more => react(unit, &StreamId::<Walker>::new(format!("walk-{}", more - 1)), Walk { left: more - 1, spread: 0 }),
        }
    }
}

fn walkers(store: &Sqlite) -> usize {
    store.read(|view: &mut View<'_>| view.states::<Walker>()).expect("read").len()
}

#[test]
fn one_fact_whose_policy_reaches_forty_streams_is_stored() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(advance(&store, &StreamId::new("hub"), Walk { left: 0, spread: 40 }), Ok(()));
    assert_eq!(walkers(&store), 41);
}

#[test]
fn a_cascade_over_fresh_streams_as_long_as_a_unit_allows_is_stored() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(advance(&store, &StreamId::new("walk-1023"), Walk { left: 1023, spread: 0 }), Ok(()));
    assert_eq!(walkers(&store), 1024);
}

#[test]
fn a_cascade_over_fresh_streams_one_outcome_longer_is_a_runaway_and_stores_nothing() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(
        advance(&store, &StreamId::new("walk-1024"), Walk { left: 1024, spread: 0 }),
        Err(StoreError::Runaway {
            lifecycle: "cascade_walker",
            outcomes: 1024
        })
    );
    assert_eq!(walkers(&store), 0);
}

#[test]
fn two_policies_that_trigger_each_other_end_the_unit_as_a_runaway_and_store_nothing() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(
        advance(&store, &StreamId::new("r-1"), Serve),
        Err(StoreError::Runaway {
            lifecycle: "cascade_ping",
            outcomes: 32
        })
    );
    assert_eq!(ping(&store), (Version::ZERO, Ping::Still));
}

#[test]
fn a_policy_that_triggers_nothing_further_applies_one_outcome() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(advance(&store, &StreamId::new("c-1"), Forge { left: 0 }), Ok(()));
    assert_eq!(chain(&store), (Version::from(1), Chain::Linked { links: 1 }));
}

#[test]
fn a_cascade_of_as_many_outcomes_as_a_unit_allows_is_stored() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(advance(&store, &StreamId::new("c-1"), Forge { left: 31 }), Ok(()));
    assert_eq!(chain(&store), (Version::from(32), Chain::Linked { links: 32 }));
}

#[test]
fn a_cascade_one_outcome_longer_is_a_runaway_and_stores_nothing() {
    let store: Sqlite = Sqlite::in_memory().expect("opens");
    assert_eq!(
        advance(&store, &StreamId::new("c-1"), Forge { left: 32 }),
        Err(StoreError::Runaway {
            lifecycle: "cascade_chain",
            outcomes: 32
        })
    );
    assert_eq!(chain(&store), (Version::ZERO, Chain::Loose));
}
