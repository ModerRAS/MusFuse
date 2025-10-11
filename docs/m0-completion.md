# MusFuse M0 基础骨架实现完成

##  已完成的功能

### ✅ 核心文件系统操作
- **文件读写**: 完全透传的文件读取和写入操作
- **目录操作**: 目录创建、读取、枚举
- **文件管理**: 文件创建、删除、重命名
- **元数据**: 文件属性查询和基本修改
- **清理操作**: 正确处理文件关闭和删除标记

### ✅ WinFSP 集成
- 实现了 `FileSystemContext` trait
- 完整的 passthrough 文件系统实现
- 正确的生命周期管理
- 挂载/卸载功能

### ✅ 架构设计
- 清晰的模块分离：
  - `adapter/passthrough.rs`: 透传文件系统核心实现
  - `adapter/host_impl.rs`: WinFSP 主机管理
  - `adapter/winfsp.rs`: WinFSP trait 定义
  - `provider.rs`: 挂载提供者
- 异步支持 (async/await)
- 错误处理和日志记录

## 📦 项目结构

```
musfuse-windows/
├── src/
│   ├── adapter/
│   │   ├── mod.rs           # 模块导出
│   │   ├── winfsp.rs        # WinFSP trait 定义
│   │   ├── passthrough.rs   # 透传文件系统实现 (核心)
│   │   └── host_impl.rs     # WinFSP 主机实现
│   ├── provider.rs          # 挂载提供者
│   ├── lib.rs              # 库入口
│   └── main.rs             # CLI 可执行文件
├── build.rs                # 构建脚本 (DELAYLOAD 设置)
├── Cargo.toml              # 依赖配置
└── README.md               # 使用文档
```

## 🚀 如何使用

### 前置要求

1. **WinFSP**: 从 https://github.com/winfsp/winfsp/releases 下载安装
2. **LLVM/Clang**: 用于编译 winfsp-sys
   ```powershell
   winget install LLVM.LLVM
   ```
3. **设置环境变量**:
   ```powershell
   $env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
   ```

### 构建

```powershell
# 开发构建
cargo build --package musfuse-windows

# 发布构建
cargo build --package musfuse-windows --release
```

### 运行

```powershell
# 基本用法
cargo run --package musfuse-windows -- --source "D:\Music" --mount "M:"

# 带详细日志
cargo run --package musfuse-windows -- --source "D:\Music" --mount "M:" --verbose

# 发布版本
.\target\release\musfuse.exe --source "D:\Music" --mount "M:"
```

### 卸载

按 `Ctrl+C` 即可安全卸载文件系统。

## 🎯 实现细节

### PassthroughFS 实现

`PassthroughFS` 实现了 WinFSP 的 `FileSystemContext` trait，提供以下核心功能：

1. **文件上下文管理**
   - 使用 `Arc<FileContext>` 共享文件状态
   - 内部使用 `RwLock<Option<fs::File>>` 按需打开文件句柄
   - 支持删除标记

2. **路径解析**
   - 将 WinFSP 路径 (UTF-16) 转换为系统路径
   - 正确处理相对路径和根目录

3. **元数据转换**
   - Windows 文件属性 → `FileInfo`
   - `SystemTime` → FILETIME (Windows 格式)

4. **目录枚举**
   - 使用 `DirBuffer` 进行高效的目录枚举
   - 支持大型目录
   - 正确处理目录标记

5. **错误处理**
   - 将 Rust `std::io::Error` 转换为 WinFSP 错误
   - 使用 NTSTATUS 代码以确保兼容性

### WinFspHostImpl 实现

管理 `FileSystemHost` 生命周期：

- 初始化 WinFSP 运行时
- 创建和配置文件系统主机
- 启动调度器
- 挂载/卸载操作
- 正确的资源清理

### 配置和参数

卷参数配置（VolumeParams）：
- 文件系统名称: "MusFuse"
- 扇区大小: 4096 字节
- 每分配单元扇区数: 1
- 最大文件名长度: 255 字符
- 不区分大小写
- 保留文件名大小写
- Unicode 支持
- 仅在修改时清理

## 📝 关键实现要点

### 1. 文件上下文的生命周期

```rust
pub struct FileContext {
    pub path: PathBuf,
    pub delete_on_close: bool,
    pub file: RwLock<Option<fs::File>>,  // 按需打开
}
```

文件句柄按需打开以节省资源，使用 `RwLock` 保证线程安全。

### 2. 目录枚举

```rust
let dir_buffer = DirBuffer::new();
let _lock = dir_buffer.acquire(marker.is_none(), None)?;

for entry in entries {
    let mut dir_info: DirInfo<255> = DirInfo::new();
    dir_info.set_name(&file_name_str)?;
    // 设置元数据...
    _lock.write(&mut dir_info)?;
}

Ok(dir_buffer.read(marker, buffer))
```

使用 WinFSP 的 `DirBuffer` API 进行高效枚举。

### 3. 错误转换

```rust
// std::io::Error 自动转换
Err(e) => Err(FspError::from(e))

// NTSTATUS 需要手动包装
Err(FspError::NTSTATUS(STATUS_DIRECTORY_NOT_EMPTY.0))
```

## 🧪 测试

当前已通过手动测试：
- ✅ 挂载到盘符 (M:)
- ✅ 文件读取
- ✅ 目录浏览
- ✅ 基本元数据查询

待添加自动化测试。

## 📌 已知限制

1. **单源目录**: 当前只支持挂载单个源目录（使用第一个配置的源）
2. **属性设置**: 文件属性修改功能简化实现
3. **安全描述符**: 不支持 ACL（设置 `persistent_acls = false`）
4. **重解析点**: 不支持符号链接和重解析点
5. **命名流**: 不支持 NTFS 命名流

这些是 M0 阶段的预期限制，将在后续迭代中改进。

## 🔄 下一步 (后续 milestone)

- [ ] 音频格式检测和转换
- [ ] CUE 文件解析和虚拟轨道
- [ ] 多源目录合并
- [ ] 元数据缓存 (sled)
- [ ] 文件监控和自动刷新
- [ ] 更完善的错误处理
- [ ] 单元测试和集成测试

## 🐛 调试技巧

### 启用详细日志

```powershell
$env:RUST_LOG = "musfuse_windows=debug,musfuse_core=debug"
cargo run --package musfuse-windows -- --source "D:\Music" --mount "M:" --verbose
```

### 检查 WinFSP 服务

```powershell
Get-Service WinFsp.Launcher
```

### 强制卸载

如果程序崩溃导致挂载点无法访问：

```powershell
# 使用资源管理器卸载或重启 Windows Explorer
Stop-Process -Name explorer -Force
Start-Process explorer
```

## 📄 许可

MIT License

---

**项目状态**: M0 基础骨架 ✅ 完成

**可用于**: 测试基本透传功能，验证架构设计

**不可用于**: 生产环境，音频转换功能尚未实现
