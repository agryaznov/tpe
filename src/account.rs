use serde::Serializer;

/// User account.
#[derive(Default, Debug, Copy, Clone)]
pub struct Account {
    /// Client ID, unique, one per client.
    pub id: u32,
    /// Total balance of the client account, including held funds.
    /// We store balances as integers for simpler operations,
    /// as only precision to 10^-4 needed,
    /// we store it as <amount>*10^4.
    /// This allows dealing with balances up to ~1.84 quadrillion (`u64::MAX/10^4`),
    /// which should be quite enough.
    pub total: u64,
    /// Total funds held for dispute.
    pub held: u64,
    /// Whether the account is locked. An account is locked if a charge back occurs.
    pub locked: bool,
}

macro_rules! ensure_unlocked {
    ($a:ident) => {
        if $a.locked {
            return Err("account is frozen".to_string());
        }
    };
}

impl Account {
    /// Creates a new client account
    pub fn new(id: u32) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
    /// Returns available balance of the account.
    pub fn available(&self) -> u64 {
        self.total.saturating_sub(self.held)
    }
    /// Deposits amount to the account.
    /// Returns new total balance upon success.
    pub fn deposit(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.total = self.total.checked_add(amount).ok_or(
            "tx makes balance overflow; such enourmous balances are not supported".to_string(),
        )?;

        Ok(self.total)
    }
    /// Withdraws amount from the account.
    /// Returns new total balance upon success.
    pub fn withdraw(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        if self.available() < amount {
            return Err(format!("insufficient available balance, acc: {:?}", &self));
        };

        self.total = self
            .total
            .checked_sub(amount)
            .ok_or("insufficient total balance".to_string())?;

        Ok(self.total)
    }
    /// Holds amount on the account.
    /// Returns new available balance upon success.
    pub fn hold(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.held = self.held.saturating_add(amount);
        Ok(self.available())
    }
    /// Releases amount on the account.
    /// Returns new available balance upon success.
    pub fn release(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.held = self.held.saturating_sub(amount);
        Ok(self.available())
    }
    /// Charges an amount back.
    /// Returns new total balance upon success.
    pub fn chargeback(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.total = self.total.saturating_sub(amount);
        self.held = self.held.saturating_sub(amount);

        self.lock();
        Ok(self.total)
    }
    /// Locks account.
    pub fn lock(&mut self) {
        self.locked = true;
    }
    /// Unlocks account.
    #[allow(dead_code)]
    pub fn unlock(&mut self) {
        self.locked = false;
    }
}

/// Helper struct for simpler Account serilization.
#[derive(Debug, serde::Serialize)]
pub struct AccountSer {
    client: u32,
    #[serde(serialize_with = "ser_amount")]
    available: u64,
    #[serde(serialize_with = "ser_amount")]
    held: u64,
    #[serde(serialize_with = "ser_amount")]
    total: u64,
    locked: bool,
}

impl From<Account> for AccountSer {
    fn from(a: Account) -> Self {
        AccountSer {
            client: a.id,
            available: a.available(),
            held: a.held,
            total: a.total,
            locked: a.locked,
        }
    }
}

/// Helper for amounts serialization.
fn ser_amount<S>(a: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let f = a % 10_000;
    let s = if f > 0 {
        let f = format!("{}", &f);
        let mut zeros = String::new();
        for c in std::iter::repeat('0').take(4 - f.len()) {
            zeros.push(c)
        }
        format!("{}.{}{}", a / 10_000, zeros, f)
            .trim_end_matches('0')
            .to_owned()
    } else {
        format!("{}", a / 10_000)
    };

    serializer.serialize_str(&s)
}
