/// Implementation of signature aggregation protocols (view change and pBFT prepare/commit) using
/// the Handel protocol. The Handel protocol itself is implemented in the nimiq-handel crate.

pub mod voting;
pub mod view_change;
pub mod pbft;
