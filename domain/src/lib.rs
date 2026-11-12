use serde::de::DeserializeOwned;
use serde::Serialize;

pub mod fulfilment;
pub mod payment;
pub mod task;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Never {}

pub trait Lifecycle: Sized + Serialize + DeserializeOwned {
    const NAME: &'static str;
    type Event: Serialize + DeserializeOwned;
    type Refusal: Serialize;
    fn initial() -> Self;
    fn evolve(self, event: &Self::Event) -> Self;
}

pub type Facts<S> = Vec<<S as Lifecycle>::Event>;

pub trait Command {
    type Reads: Lifecycle;
    type Rejection: Into<<Self::Reads as Lifecycle>::Refusal>;
    fn decide(self, reads: &Self::Reads) -> Result<Facts<Self::Reads>, Self::Rejection>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct Step<E, R> {
    pub events: Vec<E>,
    pub requests: Vec<R>,
}

impl<E, R> Step<E, R> {
    pub fn nothing() -> Self {
        Step {
            events: Vec::new(),
            requests: Vec::new(),
        }
    }
}

pub trait Reaction {
    type Reads: Lifecycle;
    type Request: Serialize + DeserializeOwned;
    fn react(self, reads: &Self::Reads) -> Step<<Self::Reads as Lifecycle>::Event, Self::Request>;
}

pub trait Projection {
    const NAME: &'static str;
    type Reads: Lifecycle;
    type Row: Serialize + DeserializeOwned;
    fn key(stream: &str, event: &<Self::Reads as Lifecycle>::Event) -> Option<String>;
    fn apply(row: Option<Self::Row>, event: &<Self::Reads as Lifecycle>::Event) -> Option<Self::Row>;
}
