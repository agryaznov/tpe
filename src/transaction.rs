use serde::{Deserialize, Deserializer};
use std::fmt::Debug;

/// Types of transactions.
/// We call first two _transactions_, as we store them into engine,
/// and we call other three _events_, as they change state of
/// transactions happened before.
#[derive(Debug, serde::Deserialize, serde::Serialize, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Tx {
    /// Credit to client account, increases its available (and therefore total) balance.
    /// This is a money-moving _transaction_.
    Deposit,
    /// Debit to client account, decreases its available (and therefore total) balance.
    /// This is money-moving _transaction_.
    Withdrawal,
    /// Client's claim to reverse an erroneous transaction.
    /// The transaction disputed is the one specified by its ID in the corresponding csv line.
    /// Therefore a dispute does not has its own transaction ID.
    /// This should result in hold of the amount of the corresponding transaction
    /// on the client's account.
    /// This is an _event_.
    Dispute,
    /// Resolution to a dispute, which is specified by ID of the transaction being disputed.
    /// This is an _event_.
    Resolve,
    /// Outcome of a dispute which is resolved positively, which is specified by ID of the transaction being disputed.
    /// This is an _event_.
    /// not
    Chargeback,
}

/// Client transaction.
/// Implemented as a simple state machine.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct Transaction {
    /// Transaction ID, unique, one per client.
    #[serde(rename = "tx", default)]
    pub id: u32,
    /// Transaction type.
    #[serde(rename = "type")]
    pub ty: Option<Tx>,
    /// ID of the client Account performing the Transaction.
    pub client: u32,
    /// Transacttion amount.
    /// We store balances as integers for simpler operations,
    /// as only precision to 10^-4 needed,
    /// we store it as <amount>*10^4.
    /// This allows dealing with balances up to ~1.84 quadrillion (`u64::MAX/10^4`),
    /// which should be quite enough.
    #[serde(default, deserialize_with = "deser_amount")]
    pub amount: Option<u64>,
    /// Transaction state.
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

// Transaction state objects.
#[derive(Default, Debug)]
pub struct Received;
#[derive(Debug)]
pub struct Executed;
#[derive(Debug)]
pub struct Disputed;
#[derive(Debug)]
pub struct Reverted;

/// Used by state objects to return their state to caller.
/// (This is done as an alternative to downcasting `<dyn TxState>`).
#[derive(Debug)]
pub enum State {
    Received,
    Executed,
    Disputed,
    Reverted,
    Undefined,
}

impl Transaction {
    pub fn init(&mut self, state: Box<dyn TxState>) -> Result<(), String> {
        self.state = Some(state);

        match self.ty {
            Some(Tx::Deposit) | Some(Tx::Withdrawal) => match self.amount {
                None | Some(0) => {
                    Err(r"deposits\withdrawals with 0 amount are ignored".to_string())
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    pub fn state(&self) -> State {
        if let Some(s) = &self.state {
            s.state()
        } else {
            State::Undefined
        }
    }

    declare_transitions!(execute, dispute, resolve, revert);
}

/// Interface for the state objects.
pub trait TxState: std::fmt::Debug {
    fn state(&self) -> State;
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
        fn state(&self) -> State {
            State::$state
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

/// Helper for amounts deserialization.
/// We deser amount to integer value = <amount>*10^4.
/// This allows balances up to ~1.84 quadrillion (`u64::MAX/10^4`),
/// which should be quite enough.
/// If requested transaction balance is >= `u64::MAX/10^4`,
/// we deseriaze it to None.
fn deser_amount<'de, D>(de: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<&str>::deserialize(de)
        .unwrap_or(None)
        .and_then(|s| {
            let v = s.split('.').take(2).collect::<Vec<_>>();
            let mut s = v[0].to_owned();
            match v.len() {
                1 => s.push_str("0000"),

                2 => match v[1].len() {
                    n @ 0..=4 => {
                        s.push_str(&v[1][0..n]);
                        for c in std::iter::repeat('0').take(4 - n) {
                            s.push(c)
                        }
                    }
                    5.. => s.push_str(&v[1][0..4]),
                },
                _ => (),
            };
            s.parse::<u64>().ok()
        }))
}
