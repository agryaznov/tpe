# Design Decisions 

+ `Transaction` is implemented as a simple state machine using typestate pattern.  
  This allows to encapsulate business logic of payment lifecycle into transaction itself, as well as to eliminate faulty state transitions in runtime, and make code more clear. Thankfully to this approach, possible bugs here are being catched at compile time.
  This also improves code maintenability as it makes modifying the rules of payment workflow a fairly simple task.
  
+ `Engine` stores transactions and client accounts in two `HashMap`s.   
  This is done for faster lookups, as we can't make assumptions on the order of the transactions coming from the input. For transactions, we only store the ones which passed sanity checks and succeed, and only one for each `Depoist` and `Withdraw` action. Other actions, namely `Dispute`, `Resolve` and `Chargeback` does not add up to memory footprint, as they just (possibly) mutate stored transaction's state. `Account` is stored only upon its first successful transaction. 

+ The Engine processes input file line-by-line, in infalible mode, dropping entries it can't read or process as required by the spec.  
  It can work just fine with faulty input files, processing and storing to memory only valid transactions. Therefore it (hopefully) can be considered resource-efficient, robust and secure.

+ Amounts are stored in `u64` to avoid rounding errors and to make operations with them less error-prone.  
  The downside of this is the need of implementing custom (de)serialization logic. This is covered by unit and integration tests to minimize the risk of bugs.  
  Maximum amount balance is thereby bounded by `u64::MAX/10_000`. Any transaction making client balance exceed this limit will fail. This is covered by tests.

+ Although in current version the engine is single-threaded, with the design taken it can be easily parallelized by using Mutexes on Accounts and splitting work between threads on per-account basis.  


see also 

```
cargo doc --open
```
