# MusFuse

MusFuse 致力于打造跨平台的虚拟音乐文件系统，通过 FUSE/WinFSP 将真实音频资产映射成结构化、可扩展的音乐库视图，实现分轨虚拟化、统一元数据覆盖与可插拔的 KV 存储层。

## 🗺️ 架构策划案

- [MusFuse 架构策划案](docs/architecture.md)

## ✨ 核心愿景

- CUE 分轨虚拟化与多格式音频策略（有损直通、无损转 FLAC）。
- 非破坏性 Tag/封面覆盖，统一存入外部 KV。
- Linux/macOS（FUSE）与 Windows（WinFSP）双平台挂载。
- 可扩展的缓存、监控与 REST/HTTP 控制面。