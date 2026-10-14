pub mod metainfo;
pub mod mock;
pub mod remote;
pub mod rqbit;
pub mod rtorrent;
pub mod traits;

pub use mock::MockTorrentEngine;
pub use remote::RemoteServerEngine;
pub use rqbit::RqbitTorrentEngine;
pub use rtorrent::RtorrentRemoteEngine;
pub use traits::{EngineHook, EngineStatus, TorrentEngine};

pub type SharedEngine = std::sync::Arc<dyn TorrentEngine>;
