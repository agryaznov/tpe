use std::fmt::Debug;

// Transaction states
#[derive(Default, Debug)]
pub struct Received;
#[derive(Debug)]
pub struct Executed;
#[derive(Debug)]
pub struct Disputed;
#[derive(Debug)]
pub struct Reverted;

#[derive(Debug)]
pub enum St {
    Received,
    Executed,
    Disputed,
    Reverted,
    Undefined,
}

/// User transaction.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Transaction {
    /// Transaction ID, unique, one per client.
    #[serde(rename = "tx")]
    pub id: u32,
    /// Transaction type.
    #[serde(rename = "type")]
    pub ty: Tx,
    /// ID of the client Account performing the Transaction.
    pub client: u32,
    /// Transacttion amount
    #[serde(default)]
    pub amount: u64,
    /// Whether the transaction is under dispute.
    #[serde(skip)]
    state: Option<Box<dyn TxState + 'static>>,
}

macro_rules! declare_transitions {
    ($($transition:ident),+) => {
            $(
            pub fn $transition(&mut self) {
                if let Some(s) = self.state.take() {
                    self.state = Some(s.$transition())
                }
            }
           )+
    };
}

impl Transaction {
    pub fn init(&mut self, state: Box<dyn TxState>) {
        self.state = Some(state)
    }

    pub fn state(&self) -> St {
        if let Some(s) = &self.state {
            s.state()
        } else {
            St::Undefined
        }
    }

    declare_transitions!(execute, dispute, resolve, revert);
}

pub trait TxState: std::fmt::Debug {
    fn state(&self) -> St;
    fn execute(self: Box<Self>) -> Box<dyn TxState>;
    fn dispute(self: Box<Self>) -> Box<dyn TxState>;
    fn resolve(self: Box<Self>) -> Box<dyn TxState>;
    fn revert(self: Box<Self>) -> Box<dyn TxState>;
}

macro_rules! impl_fallbacks {
    ($($fallback:ident),+) => {
            $(
            fn $fallback(self: Box<Self>) ->  Box<dyn TxState> {
                self
            }
           )+
    };
}

macro_rules! impl_state_getter {
    ($state:ident) => {
        fn state(&self) -> St {
            St::$state
        }
    };
}

impl TxState for Received {
    fn execute(self: Box<Self>) -> Box<dyn TxState> {
        Box::new(Executed)
    }

    impl_fallbacks!(dispute, resolve, revert);
    impl_state_getter!(Received);
}

impl TxState for Executed {
    fn dispute(self: Box<Self>) -> Box<dyn TxState> {
        Box::new(Disputed)
    }

    impl_fallbacks!(execute, resolve, revert);
    impl_state_getter!(Executed);
}
impl TxState for Disputed {
    fn resolve(self: Box<Self>) -> Box<dyn TxState> {
        Box::new(Executed)
    }

    fn revert(self: Box<Self>) -> Box<dyn TxState> {
        Box::new(Reverted)
    }

    impl_fallbacks!(execute, dispute);
    impl_state_getter!(Disputed);
}
impl TxState for Reverted {
    impl_fallbacks!(execute, dispute, resolve, revert);
    impl_state_getter!(Reverted);
}

/// Types of Transactions.
#[derive(Debug, serde::Deserialize, serde::Serialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Tx {
    /// Credit to client account, increases its available (and therefore total) balance.
    Deposit,
    /// Debit to client account, decreases its available (and therefore total) balance.
    Withdrawal,
    /// Client's claim to reverse an erroneous transaction.
    /// The transaction disputed is the one specified by its ID in the corresponding csv line.
    /// Therefore a dispute does not has its own transaction ID.
    /// This should result in hold of the amount of the corresponding transaction
    /// on the client's account.
    Dispute,
    /// Resolution to a dispute, which is specified by ID of the transaction being disputed.
    Resolve,
    /// Outcome of a dispute which is resolved positively, which is specified by ID of the transaction being disputed.
    Chargeback,
}

impl Default for Tx {
    fn default() -> Self {
        Tx::Resolve
    }
}
