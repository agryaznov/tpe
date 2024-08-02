/// User account
#[derive(Default, Debug, Copy, Clone)]
pub struct Account {
    /// Client ID, unique, one per client.
    pub id: u32,
    /// Total balance of the client account, including held funds.
    /// We store balances as integers for simpler operations,
    /// as only precision to 10^-5 needed,
    /// we store it as <amount>*10^5.
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
            return Err(format!("insufficient available balance, acc: {:?}", &self).to_string());
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

        // TODO: might be an attack vector?
        self.held = self.held.saturating_add(amount);
        Ok(self.available())
    }
    /// TODO
    pub fn release(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.held = self.held.saturating_sub(amount);
        Ok(self.available())
    }
    /// TODO should we check that this happens AFTER dispute?
    /// Chargeback is possible only if a dispute has been opened for the transaction in question.
    pub fn chargeback(&mut self, amount: u64) -> Result<u64, String> {
        ensure_unlocked!(self);

        self.total = self.total.saturating_sub(amount);
        self.held = self.held.saturating_sub(amount);

        self.lock();
        Ok(self.available())
    }
    pub fn lock(&mut self) {
        self.locked = true;
    }
    /// TODO
    #[allow(dead_code)]
    pub fn unlock(&mut self) {
        self.locked = false;
    }
}
