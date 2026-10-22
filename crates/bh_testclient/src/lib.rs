//! Protocol test client (game-client stand-in) and parity diff harness for the Blockheads server.
//!
//! Owned by test_client (the `harness` module belongs to diff_harness).

pub mod client;
pub mod decode;
pub mod harness;
pub mod modded;
pub mod scenario;
pub mod session;
pub mod trace;

pub use client::{Client, ClientConfig, NetEvent, Rx};
pub use decode::{decode_packet, pretty, type_name, Decoded};
pub use modded::{Download, LoaderOptions, ModClient};
pub use scenario::{BlockheadRef, RunOptions, RunReport, Scenario, ScenarioError, Step};
pub use session::{
    BlockheadSpec, CreatedObject, FillNetRequest, GatherNetRequest, Identity, JoinOutcome, PaintNetRequest,
    PlaceInteractionObjectNetRequest, PlaceWorkbenchNetRequest, PlistDialect, ProjectileNetRequest,
    RemoveBackWallNetRequest, RemoveNetRequest, Session, SessionConfig, SessionState, WorldHeartbeat,
};
pub use trace::{read_trace, write_record, Dir, TraceRecord, TraceSink};
