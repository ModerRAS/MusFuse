# MusFuse 快速启动指南

## 快速测试 M0 基础骨架

### 步骤 1: 设置环境

```powershell
# 设置 LLVM 路径（如果还没有设置）
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
```

### 步骤 2: 构建项目

```powershell
# 进入项目目录
cd D:\WorkSpace\Rust\MusFuse

# 构建 (第一次可能需要几分钟)
cargo build --package musfuse-windows --release
```

### 步骤 3: 准备测试目录

创建一个测试目录并放入一些文件：

```powershell
# 创建测试目录
New-Item -ItemType Directory -Path "D:\TestMusic" -Force

# 复制一些测试文件
Copy-Item "C:\Windows\Media\*.wav" "D:\TestMusic\"
```

### 步骤 4: 挂载文件系统

```powershell
# 挂载到 M: 盘符
.\target\release\musfuse.exe --source "D:\TestMusic" --mount "M:"

# 或者使用 cargo run (开发模式)
cargo run --package musfuse-windows -- --source "D:\TestMusic" --mount "M:"
```

你应该会看到：

```
MusFuse starting...
Source: "D:\\TestMusic"
Mount point: "M:"
WinFSP is installed and initialized
mounting source: "D:\\TestMusic" to "M:"
mounting to: M:
filesystem mounted successfully to M:
Filesystem mounted successfully!
Press Ctrl+C to unmount and exit...
```

### 步骤 5: 测试文件系统

打开新的 PowerShell 窗口：

```powershell
# 查看挂载的驱动器
Get-PSDrive M

# 列出文件
dir M:\

# 读取文件
Get-Content M:\chord.wav | Measure-Object -Line

# 复制文件测试
Copy-Item "M:\chord.wav" "D:\test_copy.wav"

# 创建新文件测试
"Test Content" | Out-File "M:\test.txt"
Get-Content "M:\test.txt"

# 创建目录测试
New-Item -ItemType Directory -Path "M:\TestFolder"
dir M:\
```

### 步骤 6: 卸载

回到运行 MusFuse 的窗口，按 `Ctrl+C`:

```
Received Ctrl+C, unmounting...
unmounting: "M:"
filesystem unmounted successfully
Filesystem unmounted successfully
```

## 常见问题排查

### 问题 1: "failed to initialize WinFSP"

**解决方案**: 
```powershell
# 检查 WinFSP 服务
Get-Service WinFsp.Launcher

# 如果未运行，启动它（需要管理员权限）
Start-Service WinFsp.Launcher
```

### 问题 2: "couldn't find any valid shared libraries matching: ['clang.dll', 'libclang.dll']"

**解决方案**:
```powershell
# 设置环境变量
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"

# 或永久设置（需要管理员权限）
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "Machine")
```

### 问题 3: "mount point is in use" 或驱动器无法访问

**解决方案**:
```powershell
# 如果程序异常终止，驱动器可能仍然被占用
# 重启 Windows Explorer
Stop-Process -Name explorer -Force
Start-Process explorer

# 或重新启动计算机
```

### 问题 4: 权限拒绝

**解决方案**: 确保源目录有读写权限

```powershell
# 检查权限
Get-Acl "D:\TestMusic" | Format-List
```

## 性能提示

1. **使用发布构建**: `--release` 标志可以显著提升性能
2. **详细日志会降低性能**: 仅在调试时使用 `--verbose`
3. **大文件操作**: 当前实现对大文件 (>1GB) 的性能可能不是最优

## 下一步

现在您已经验证了 M0 基础骨架可以工作，接下来可以：

1. 查看 `docs/m0-completion.md` 了解实现细节
2. 检查 `crates/musfuse-windows/README.md` 获取完整文档
3. 开始开发 M1 milestone 的功能（音频格式转换）

## 需要帮助？

如果遇到问题：

1. 检查 `scripts/check-windows-env.ps1` 脚本的输出
2. 启用详细日志: 添加 `--verbose` 标志
3. 查看 WinFSP 日志（如果有的话）

祝测试愉快！ 🎉
