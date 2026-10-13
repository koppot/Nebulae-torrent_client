use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdaptiveSwarmConfig {
    pub swarm_size: u32,
    pub active_connections_limit: u32,
    pub pipeline_queue_depth: u32,
    pub socket_buffer_bytes: u32,
    pub endgame_active: bool,
    pub disk_coalescing_mb: u32,
}

impl AdaptiveSwarmConfig {
    pub fn auto_tune(
        swarm_size: u32,
        avg_rtt_ms: u32,
        measured_rate_bps: u64,
        remaining_pieces: u32,
    ) -> Self {
        // 1. Connection limits based on swarm size
        let active_connections_limit = if swarm_size < 10 {
            swarm_size.max(4)
        } else if swarm_size < 100 {
            40
        } else if swarm_size < 500 {
            80
        } else {
            120
        };

        // 2. Bandwidth-Delay Product (BDP)
        // BDP = Rate (bytes/s) * RTT (seconds)
        let rtt_sec = (avg_rtt_ms.max(10) as f64) / 1000.0;
        let bdp_bytes = (measured_rate_bps as f64 * rtt_sec) as u32;

        // Socket buffer bounded between 64 KiB and 2 MiB
        let socket_buffer_bytes = bdp_bytes.clamp(64 * 1024, 2 * 1024 * 1024);

        // 3. Request pipelining depth (standard block size = 16 KiB)
        let pipeline_queue_depth = (bdp_bytes / 16384).clamp(4, 32);

        // 4. Endgame mode: activates when <= 5 pieces remain
        let endgame_active = remaining_pieces <= 5 && remaining_pieces > 0;

        // 5. Coalescing write buffer: 16 MiB default, scales up to 64 MiB for gigabit rates
        let disk_coalescing_mb = if measured_rate_bps > 50 * 1024 * 1024 {
            64
        } else {
            16
        };

        Self {
            swarm_size,
            active_connections_limit,
            pipeline_queue_depth,
            socket_buffer_bytes,
            endgame_active,
            disk_coalescing_mb,
        }
    }
}
