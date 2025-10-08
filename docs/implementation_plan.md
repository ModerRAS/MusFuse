# MusFuse 核心功能实现计划

## 目标概述

围绕第一阶段“可挂载的 Windows 原型”，重点实现以下四类核心能力：

1. **KV 元数据存储层**：统一保存 Track/Album 元数据、封面、扫描缓存和策略配置。
2. **Tag 读写管线**：从物理音频读取内嵌标签，叠加数据库覆盖写入，提供非破坏性更新能力。
3. **Cue 分轨与 Track 映射**：解析 CUE + 音频文件，对外导出虚拟 Track 视图并维护索引。
4. **音频读取与转码**：基于策略对无损音频转 FLAC 流输出，支持 lossy passthrough。

### 假设与约束

- 当前阶段聚焦单机 Windows 平台（WinFSP），Linux/macOS 推迟至后续迭代。
- 首个可运行版本仅支持本地文件系统源，不处理网络共享或云端对象存储。
- 默认 KV 后端使用 `sled`，但会设计 trait 以便未来替换 RocksDB/SQLite/Redis。
- 音频解析依赖 `symphonia`，转码采用 `claxon` (FLAC encoder) 或 `libflac` FFI，视性能需求调整。
- Tag 读写使用 `lofty`，只覆盖常见格式（FLAC/APE/WAV/MP3/AAC/OGG）。
- Cue 解析使用 `cue-parser` crate；若功能不足再考虑自研。

## 功能拆解

| 能力 | 子任务 | 说明 |
| ---- | ------ | ---- |
| KV 层 | 键空间定义、模型序列化、sled 实现、Mock 实现 | 为后续测试提供内存 Mock | 
| Tag 管线 | `TagReader`, `TagOverlay`, `TagWriter` | 多格式兼容，写入只影响 KV，不改原文件 |
| Cue/Track | `CueParser`, `TrackMapper`, `DirectoryScanner` | 支持增量扫描与缓存 |
| 媒体引擎 | `AudioReader`, `FormatTranscoder`, `CoverExtractor` | 实现策略化输出与封面提取 |
| 集成 | `MediaEngine`, `MetaCollector`, `FileRouter` | 将各模块组合并对接挂载层 |

## 迭代里程碑

- **M0 (当前)**：完成 WinFSP Adapter + Provider mock 测试，通过。
- **M1**：落地 KV 层 + Tag 管线基础接口，提供单元测试。
- **M2**：实现 Cue/Track pipeline 与目录扫描。
- **M3**：实现音频读取/转码及封面提取。
- **M4**：整合所有模块至挂载流程，完成端到端 e2e 测试（使用虚拟文件）。

## 数据模型初稿

- `TrackMetadata`：title, artist, album, disc, index, duration, tags(JSON)。
- `AlbumMetadata`：album, artist, year, cover_ref, tracks(Vec<TrackId>)。
- `CueCache`：原始 CUE hash、解析结果、track offsets。
- `FileStat`：path hash, mtime, size, checksum。

键空间参照 `docs/architecture.md` 中约定，新增转码缓存、封面缓存键：

```
track:{id}:transcode -> { "format": "FLAC", "path": "..." }
cover:{id}:hash -> sha1
cache:transcode:{hash} -> binary FLAC blob
```

## 风险与预案

- **音频转码性能**：先实现同步版本，若性能不足再引入 rayon/tokio blocking。
- **Cue 与音频匹配**：引入哈希校验，避免文件名差异导致映射错误。
- **Tag 格式兼容**：为不支持的格式提供 graceful fallback，返回 `Unsupported`。

## 测试策略

- 单元测试覆盖数据模型序列化、KV 键操作、Tag overlay 合并逻辑。
- 引入 fixture 目录存放示例 cue+音频+tag 文件，用于集成测试。
- 模拟 WinFSP Host 进行端到端读写测试（无需真实驱动）。
