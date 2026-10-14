pub mod failed;
pub mod killswitch;
pub mod metainfo;
pub mod mock;
pub mod rqbit;
pub mod seeding;
pub mod traits;

pub use failed::FailedTorrentEngine;
pub use mock::MockTorrentEngine;
pub use rqbit::RqbitTorrentEngine;
pub use traits::{EngineHook, EngineStatus, TorrentEngine};

pub type SharedEngine = std::sync::Arc<dyn TorrentEngine>;
