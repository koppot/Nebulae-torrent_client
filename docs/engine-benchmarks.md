# Nebulae — Engine Benchmark Gate & Soak Testing Methodology

This document establishes the empirical qualification standards for Phase 1 of the Nebulae BitTorrent client. Phase 1 is certified as complete only when the embedded engine (`librqbit v9.0.1`) satisfies the benchmark criteria under realistic Asian DVD Club (ADC) and high-concurrency seeding workloads.

---

## 1. Benchmark Gate Objective & Scope

The objective of the Benchmark Gate is to verify that `librqbit` sustains high-capacity desktop and seedbox operations without memory growth, file descriptor exhaustion, tracker protocol violations, or peer degradation.

Soak tests and benchmarks are executed locally on dedicated hardware or seedbox environments (never in resource-constrained CI virtual machines). Dated execution logs and metrics are archived alongside this specification.

---

## 2. Benchmark Workload Profile

The testing suite subjects the engine to a multi-tiered stress profile:

| Parameter | Specification | Target Validation |
| :--- | :--- | :--- |
| **Concurrent Torrents** | **100+ active torrents** | Scalability of session tracking, event loop efficiency |
| **Swarm Composition** | Mixed: 70 completed seeds, 30 active partial downloads | Bi-directional I/O, simultaneous seeding & downloading |
| **Swarm Types** | Mixed: $\ge 1$ Private BEP 27 swarm (ADC) + $\ge 1$ Public swarm | BEP 27 private restrictions vs. public DHT/PEX concurrency |
| **Peer Density** | Hundreds of connected peers across active swarms | Socket management, choking/unchoking algorithms |
| **Duration (Short-run)** | Continuous 4-hour high-throughput burst | Memory ceiling, disk cache flushing, throughput saturation |
| **Duration (Long-run)** | **72-hour continuous soak test** | Zero FD leakage, long-term memory stability, zero deadlocks |

---

## 3. Telemetry & Measurement Framework

Telemetry is collected continuously via `librqbit`'s built-in Prometheus metrics export and local HTTP inspection endpoints (`http://127.0.0.1:3030/api/v1/stats`):

### 3.1. Primary Verification Metrics

1. **Memory Ceiling**:
   - *Target*: Steady-state Resident Set Size (RSS) $\le 250\text{ MiB}$ during 100-torrent seeding; $\le 512\text{ MiB}$ under gigabit active downloading.
   - *Requirement*: Linear memory growth is strictly prohibited. Memory must plateau after initial chunk buffers and file handles stabilize.

2. **File Descriptor (FD) Stability**:
   - *Target*: Constant open file descriptor count matching active connections + open chunk handles.
   - *Requirement*: Zero descriptor leakage over the 72-hour soak. All closed peer sockets and completed file handles must be immediately released.

3. **Ratio & Upload Accounting Fidelity**:
   - *Target*: Exact mathematical agreement between `uploaded_bytes` / `downloaded_bytes` recorded by `librqbit`, persisted across restarts, and reflected in ADC tracker reports.
   - *Requirement*: Restarts, unexpected termination, or engine reloads must not reset or corrupt historical transfer counters.

4. **Fast-Resume Integrity (No Rehash on Restart)**:
   - *Target*: Session restart resumes 100 torrents in $< 3\text{ seconds}$ without triggering hash re-checks.
   - *Requirement*: Bitfield caches in `internal/engine-state/` must accurately preserve validated piece bitfields.

5. **Tracker Announce Cadence & Passkey Safety**:
   - *Target*: Strict compliance with tracker `interval` and `min_interval` parameters. Zero rapid re-announce loops.
   - *Requirement*: All private tracker announce URLs with passkeys must remain strictly redacted in disk logs (`SafeLogger`).

6. **CPU Utilization Ceiling**:
   - *Target*: $\le 5\%$ average CPU utilization during steady-state 100-torrent seeding on modern 4-core desktop hardware.

---

## 4. Swarm Efficiency & Escalation Policy

### 4.1. Piece Availability Contribution
Because media clients frequently employ sequential piece selection for immediate preview, the benchmark monitors swarm piece health. If sequential downloading causes measurable piece availability starvation across peers in test swarms, the engine's piece picker strategy must be tuned.

### 4.2. Escalation Trigger
If any of the following failure modes occur during qualification:
1. Unresolved memory leak or file-descriptor exhaustion across the 72-hour soak test;
2. Uncorrectable state corruption during fast-resume checkpoint restoration;
3. Inability of `librqbit` to sustain 100+ concurrent swarms without thread pool starvation;
4. Upstream abandonment or refusal of critical security/protocol patches;

The project will immediately escalate to the designated fallback engine (**libtorrent-rasterbar**) as detailed in [`docs/architecture.md`](file:///Users/dev/.gemini/antigravity/scratch/nebula-torrent/docs/architecture.md#48-fallback-engine-policy-libtorrent-rasterbar).

---

## 5. Qualification Test Results Log

### Run ID: `SOAK-2026-10-14-01`
- **Date**: 2026-10-14
- **Environment**: macOS 15.x / Apple Silicon (ARM64) & Linux x86_64 Seedbox
- **Engine Version**: `librqbit v9.0.1` (`rust-tls`)
- **Initial Verification**:
  - [x] Initial engine startup and listener binding on `listening_port` (51413).
  - [x] Fast-resume state checkpointing to `internal/engine-state/`.
  - [x] BEP 27 private swarm restrictions active (DHT/PEX/LSD blocked on ADC test torrents).
  - [x] Passkey redaction verified in session logs.
  - [ ] 72-Hour continuous soak test: *In Progress (scheduled for staging environment)*.
