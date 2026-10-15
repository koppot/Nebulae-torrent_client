pub mod failed;
pub mod killswitch;
pub mod metainfo;
pub mod mock;
pub mod remote;
pub mod rqbit;
pub mod rss;
pub mod rtorrent;
pub mod seeding;
pub mod telemetry;
pub mod traits;
pub mod transmission;
pub mod watcher;

pub use failed::FailedTorrentEngine;
pub use mock::MockTorrentEngine;
pub use remote::RemoteServerEngine;
pub use rqbit::RqbitTorrentEngine;
pub use rtorrent::RtorrentRemoteEngine;
pub use traits::{EngineHook, EngineStatus, TorrentEngine};
pub use transmission::TransmissionTorrentEngine;

pub type SharedEngine = std::sync::Arc<dyn TorrentEngine>;
