# MusFuse

MusFuse 致力于打造跨平台的虚拟音乐文件系统，通过 FUSE/WinFSP 将真实音频资产映射成结构化、可扩展的音乐库视图，实现分轨虚拟化、统一元数据覆盖与可插拔的 KV 存储层。

## 🗺️ 架构策划案

- [MusFuse 架构策划案](docs/architecture.md)

## ✨ 核心愿景

- CUE 分轨虚拟化与多格式音频策略（有损直通、无损转 FLAC）。
- 非破坏性 Tag/封面覆盖，统一存入外部 KV。
- Linux/macOS（FUSE）与 Windows（WinFSP）双平台挂载。
- 可扩展的缓存、监控与 REST/HTTP 控制面。

## 📦 项目结构

当前代码库采用 Cargo Workspace 分层：

- `crates/musfuse-core`：跨平台共享内核，定义配置、错误、策略、`MountProvider` 及 `PlatformAdapter` 等抽象。
- `crates/musfuse-windows`：Windows 平台实现，注入 WinFSP 适配器，提供状态管理与事件广播。

## 🧪 Windows 平台 TDD 流程

开发 Windows 子系统时遵循测试驱动：

```powershell
cargo test -p musfuse-windows
```

单元测试围绕 `WindowsMountProvider`，验证以下要点：

- 调用顺序：`prepare_environment` → `mount`。
- 状态迁移：Mounted / Unmounted / Faulted。
- 事件通知：通过 `broadcast` 通道分发 `MountEvent`。

利用 `mockall` 注入 WinFSP 适配器 Mock，可在纯 Windows 开发环境下快速迭代而无需真正挂载驱动。