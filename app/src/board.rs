use ports::{Journal, StoreError, View};

use crate::payments::{payments_in, PaymentStatus};
use crate::tasks::{task_board_in, TaskBoard};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub tasks: TaskBoard,
    pub payments: Vec<PaymentStatus>,
}

pub fn board<J: Journal>(journal: &J) -> Result<Board, StoreError> {
    journal.read(|view: &mut View<'_>| {
        Ok(Board {
            tasks: task_board_in(view)?,
            payments: payments_in(view)?,
        })
    })
}
