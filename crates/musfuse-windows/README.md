# MusFuse Windows 构建说明

## 前置要求

### 1. 安装 WinFSP

从 [WinFSP 官网](https://github.com/winfsp/winfsp/releases) 下载并安装最新版本的 WinFSP。

推荐版本：WinFSP 2.0 或更高

### 2. 安装 LLVM (用于 bindgen)

winfsp-sys 需要使用 bindgen 生成 Rust 绑定，这需要 LLVM/Clang。

#### 选项 A: 使用 winget 安装

```powershell
winget install LLVM.LLVM
```

#### 选项 B: 手动下载安装

1. 访问 [LLVM 下载页面](https://releases.llvm.org/)
2. 下载适合您系统的预编译版本
3. 安装后，将 LLVM 的 bin 目录添加到 PATH 环境变量

#### 选项 C: 设置 LIBCLANG_PATH

如果已经安装了 LLVM，可以设置环境变量：

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

### 3. 验证环境

```powershell
# 检查 WinFSP 是否安装
Get-Service WinFsp.Launcher

# 检查 LLVM 是否可用
clang --version
```

## 构建项目

### 构建库

```powershell
cargo build --package musfuse-windows
```

### 构建可执行文件

```powershell
cargo build --package musfuse-windows --bin musfuse
```

### 发布构建

```powershell
cargo build --package musfuse-windows --release
```

## 运行

### 基本用法

```powershell
# 开发模式
cargo run --package musfuse-windows -- --source "C:\Music" --mount "M:"

# 发布模式
.\target\release\musfuse.exe --source "C:\Music" --mount "M:"
```

### 参数说明

- `--source, -s`: 源目录路径（必需）
- `--mount, -m`: 挂载点，可以是盘符（如 `M:`）或目录路径（必需）
- `--verbose, -v`: 启用详细日志输出

### 示例

```powershell
# 挂载到盘符 M:
musfuse.exe --source "D:\MyMusic" --mount "M:"

# 挂载到目录（需要目录存在且为空）
musfuse.exe --source "D:\MyMusic" --mount "C:\MountPoint"

# 启用详细日志
musfuse.exe --source "D:\MyMusic" --mount "M:" --verbose
```

## 卸载

按 `Ctrl+C` 停止程序即可卸载文件系统。

## 故障排除

### 错误: "couldn't find any valid shared libraries matching: ['clang.dll', 'libclang.dll']"

**解决方案**: 安装 LLVM 并确保 clang.dll 在 PATH 中，或设置 LIBCLANG_PATH 环境变量。

### 错误: "failed to initialize WinFSP"

**解决方案**: 
1. 确认 WinFSP 已正确安装
2. 以管理员权限运行 WinFSP 安装程序
3. 重启计算机

### 错误: "Source directory does not exist"

**解决方案**: 确认源目录路径正确且存在。

### 错误: "failed to mount filesystem"

**解决方案**:
1. 确认挂载点未被占用
2. 如果使用盘符，确认该盘符未被使用
3. 如果使用目录挂载，确认目录存在且为空
4. 尝试以管理员权限运行

## 开发模式

### 运行测试

```powershell
cargo test --package musfuse-windows
```

### 代码格式化

```powershell
cargo fmt --package musfuse-windows
```

### 代码检查

```powershell
cargo clippy --package musfuse-windows
```

## 项目结构

```
musfuse-windows/
├── src/
│   ├── adapter/
│   │   ├── mod.rs          # 适配器模块导出
│   │   ├── winfsp.rs       # WinFSP trait 定义
│   │   ├── passthrough.rs  # 透传文件系统实现
│   │   └── host_impl.rs    # WinFSP 主机实现
│   ├── provider.rs         # 挂载提供者
│   ├── lib.rs             # 库入口
│   └── main.rs            # 可执行文件入口
├── build.rs               # 构建脚本
└── Cargo.toml            # 包配置
```

## 功能说明

当前实现（M0 基础骨架）支持：

- ✅ 文件和目录的读取
- ✅ 文件和目录的创建
- ✅ 文件和目录的删除
- ✅ 文件和目录的重命名
- ✅ 文件的读写操作
- ✅ 目录枚举
- ✅ 文件属性查询和修改
- ✅ 完全透传（passthrough）模式

未来计划：

- ⏳ 音频格式转换
- ⏳ CUE 文件支持
- ⏳ 元数据缓存
- ⏳ 多源目录合并
- ⏳ 文件监控和自动刷新

## 许可证

MIT License
