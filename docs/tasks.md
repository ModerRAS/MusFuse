# MusFuse 进度追踪

## 里程碑概览

| 里程碑 | 状态 | 关键进展 | 未完事项 |
| ------ | ---- | -------- | -------- |
| **M0 基础骨架** | ✅ 完成 | `musfuse-windows` 内已实现 `WindowsMountProvider`、`WinFspAdapter`，并以 `mockall` 覆盖挂载/卸载/故障流测试。 | CLI/守护进程外壳待补充。 |
| **M1 KV + 标签管线** | 🟡 进行中 | `KvStore` + `SledBackend` 支持 `put/get/scan`，`TagOverlay` + `KvTagPersistence` 已提供覆盖写入能力（含单元测试）。 | 真实 `TagReader`、`MetaCollector`、KV 命名空间规范化工具函数尚未交付。 |
| **M2 Cue/Track 管线** | 🟡 进行中 | `CueParser` 可解析文件/字符串，`TrackMapper` 生成 `TrackIndex` (含测试)。 | `DirectoryScanner` 仍为 Trait，缺少增量扫描实现与 KV 索引持久化。 |
| **M3 媒体引擎** | 🟡 进行中 | 默认转码器已完成无损转 FLAC、分块流式输出与时间戳推导；`DefaultCoverExtractor` 支持嵌入/外部封面；新增 `MediaEngine::open_stream` 串联音频与封面流，并附带端到端测试。 | 与元数据覆盖、缓存命名空间的集成尚未落地；lossy passthrough 场景测试待扩充。 |
| **M4 平台整合** | 🔵 未启动 | — | 等待 MediaEngine + MetaCollector 完成后再推进。 |

## 已交付成果

- Windows 平台挂载骨架：`crates/musfuse-windows/src/provider.rs`、`adapter/winfsp.rs` 均已通过单元测试覆盖主要状态迁移。
- 音频核心：`DefaultFormatTranscoder` 支持无损转码、块级输出及时间戳；`MediaEngine` 完成音频转码 + 封面提取的统一入口（`media::tests::media_engine_streams_chunks_with_artwork` 验证）。
- 封面处理：`DefaultCoverExtractor` 优先嵌入封面，回退搜索 `cover.*` / `folder.*`，并具有测试用例。
- Cue/Track：`CueParser` 与 `TrackMapper` 组合可从 `.cue` 生成虚拟轨道元信息。
- KV + 标签覆盖：`KvStore` 底座实现 JSON 序列化存储，`KvTagPersistence` + `TagOverlay` 提供标签覆盖读取/写入流程（使用 `sled` 临时目录测试）。

## 进行中的工作

- 元数据集成：规划 `MetaCollector` 以结合 `TagOverlay`、KV Artwork 命名空间、MediaEngine 输出。
- 音频策略完善：补充 lossy 格式直通测试（MP3/AAC/OGG）并覆盖异常分支（缺失声道/采样率）。
- 扫描器实现：基于 `LibraryScanner` Trait 落地目录全量/增量扫描，并与 KV 建立索引缓存。

## 优先待办

1. 在 `crates/musfuse-core/src/metadata.rs` 引入 `MetaCollector`，同时扩展 `TranscodeResult` 持久化封面缓存（`KvNamespace::Artwork`）。
2. 为 `DefaultFormatTranscoder` 增加 lossy 文件样例测试，确保时间戳与块逻辑在可变比特率下正确。
3. 实现 `LibraryScanner` 的 sled 缓存持久化以及文件变动事件桥接（配合后续 mount runtime）。
4. 在 `musfuse-windows` 中接入新的 `MediaEngine` 构造，打通挂载流程的音频/元数据服务注入。

## 测试与质量现状

- `cargo test` 全量通过；新增用例覆盖音频分块有序性与封面提取成功路径。
- 尚缺乏对异常路径（解码失败、文件缺失、KV I/O 错误）的系统性测试，应在后续任务中补齐。

## 风险与注意事项

- 转码目前使用 `spawn_blocking` 单线程执行，缺乏并发节流；随着 MediaEngine 接入需评估多挂载场景的 CPU 占用。
- KV 层尚未实现 Artwork 缓存的哈希策略，存在重复存储风险。
- 目录扫描未实现，挂载后缺乏自动索引刷新机制，需要提前规划文件系统通知或定时刷新策略。
