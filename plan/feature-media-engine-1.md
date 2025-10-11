---
goal: Media Engine and Metadata Integration Plan
version: 1.0
date_created: 2025-10-10
last_updated: 2025-10-10
owner: MusFuse Core Team
status: In progress
tags: [feature, media, metadata]
---

# Introduction

![Status: In progress](https://img.shields.io/badge/status-In%20progress-yellow)

This plan defines the deterministic steps required to complete the MusFuse media engine streaming pipeline and metadata overlay integration while keeping parity with the architecture blueprint.

## 1. Requirements & Constraints

- **REQ-001**: Stream audio in 256 KiB chunks with monotonic timestamps for compatibility with FUSE/WinFSP backends.
- **REQ-002**: Surface embedded or external album artwork via a unified cover extraction service.
- **SEC-001**: Limit file system access to paths derived from `SourceTrack` inputs to avoid arbitrary reads.
- **PER-001**: Maintain lossless-to-FLAC transcoding throughput within a single blocking thread per request.
- **CON-001**: Preserve Windows-first compatibility with synchronous WinFSP adapter expectations.
- **GUD-001**: Reuse existing trait-based abstractions (`FormatTranscoder`, `CoverExtractor`, `TagOverlayService`) for dependency injection.
- **PAT-001**: Persist overlays and artwork via the centralized `KvStore` namespacing scheme (`KvNamespace::Track`, `KvNamespace::Artwork`).

## 2. Implementation Steps

### Implementation Phase 1

- GOAL-001: Stabilize the media streaming layer and expose reusable constructors.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-001 | Refactor `crates/musfuse-core/src/media.rs::DefaultFormatTranscoder` to emit `AudioChunk` vectors split into 256 KiB blocks with timestamp metadata for both passthrough and FLAC pipelines. | ✅ | 2025-10-10 |
| TASK-002 | Implement `DefaultCoverExtractor::extract` in `crates/musfuse-core/src/media.rs` to prioritise embedded artwork via `lofty::read_from_path` and fall back to `cover.*` or `folder.*` files in the source directory. | ✅ | 2025-10-10 |
| TASK-003 | Introduce `MediaEngine` in `crates/musfuse-core/src/media.rs` with fields `transcoder: Arc<dyn FormatTranscoder>` and `cover: Arc<dyn CoverExtractor>`, plus method `open_stream(&self, track: &SourceTrack, policy: AudioFormatPolicy) -> Result<TranscodeResult>` that attaches extracted artwork bytes alongside audio chunks. | ✅ | 2025-10-10 |
| TASK-004 | Export constructors `DefaultFormatTranscoder::new` and `DefaultCoverExtractor::new` through `crates/musfuse-core/src/lib.rs` and `crates/musfuse-core/src/prelude.rs`, updating `crates/musfuse-windows/src/provider.rs` to consume the new re-exports. | ✅ | 2025-10-10 |
| TASK-005 | Add `#[tokio::test]` E2E streaming test in `crates/musfuse-core/src/media.rs` validating chunk ordering, timestamp monotonicity, and artwork presence within the new `MediaEngine` workflow. | ✅ | 2025-10-10 |

### Implementation Phase 2

- GOAL-002: Integrate metadata overlays and KV-backed artwork caching with the streaming layer.

| Task | Description | Completed | Date |
|------|-------------|-----------|------|
| TASK-006 | Implement `MetaCollector` struct in `crates/musfuse-core/src/metadata.rs` exposing `load_overlay`, `apply_delta`, and `evict` methods that compose `TagOverlayService`, `KvStore`, and `CoverExtractor` outputs. |  |  |
| TASK-007 | Extend `crates/musfuse-core/src/media.rs::TranscodeResult` to include `artwork: Option<Vec<u8>>`, populating it within `MediaEngine::open_stream` by resolving cached blobs from `KvNamespace::Artwork` via `KvStore`. |  |  |
| TASK-008 | Persist artwork and overlay hashes in `crates/musfuse-core/src/kv/sled_backend.rs` by adding helper `put_artwork_blob(track: &TrackId, data: &[u8])` and ensuring `scan_prefix` tests cover the new namespace. |  |  |

## 3. Alternatives

- **ALT-001**: Stream full files as a single buffer; rejected due to excessive memory pressure on large albums.
- **ALT-002**: Use FFmpeg bindings for cover extraction; discarded to avoid external runtime dependencies and keep pure Rust tooling.

## 4. Dependencies

- **DEP-001**: `lofty` crate for embedded artwork parsing; version pinned through workspace `Cargo.lock`.
- **DEP-002**: `symphonia` codec bundle for audio decoding and transcoding readiness.

## 5. Files

- **FILE-001**: `crates/musfuse-core/src/media.rs` — housing transcoding, cover extraction, and upcoming media engine orchestration.
- **FILE-002**: `crates/musfuse-core/src/metadata.rs` — destination for the `MetaCollector` overlay orchestration logic.
- **FILE-003**: `crates/musfuse-core/src/kv/sled_backend.rs` — persistence layer requiring artwork cache helpers.
- **FILE-004**: `crates/musfuse-core/src/prelude.rs` and `crates/musfuse-core/src/lib.rs` — re-export surfaces consumed by downstream crates.

## 6. Testing

- **TEST-001**: `media::tests::media_engine_streams_chunks` — validate chunk count, timestamp order, and end-of-stream markers.
- **TEST-002**: `metadata::tests::meta_collector_persists_overlay` — confirm overlay persistence and artwork retrieval with `KvNamespace::Artwork`.

## 7. Risks & Assumptions

- **RISK-001**: Blocking FLAC encoding could stall the runtime if multiple long tracks transcode concurrently; mitigation via semaphore throttling in follow-up releases.
- **ASSUMPTION-001**: All source tracks provide sample rate and channel metadata, enabling timestamp derivation without additional probing.

## 8. Related Specifications / Further Reading

- [docs/architecture.md](../docs/architecture.md)
- [docs/implementation_plan.md](../docs/implementation_plan.md)
