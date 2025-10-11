# MusFuse Windows 环境检查脚本

Write-Host "MusFuse Windows 环境检查" -ForegroundColor Cyan
Write-Host "========================" -ForegroundColor Cyan
Write-Host ""

$allOk = $true

# 检查 WinFSP
Write-Host "[1/3] 检查 WinFSP..." -ForegroundColor Yellow
$winfspService = Get-Service -Name "WinFsp.Launcher" -ErrorAction SilentlyContinue
if ($winfspService) {
    if ($winfspService.Status -eq "Running") {
        Write-Host "  ✓ WinFSP 已安装且正在运行" -ForegroundColor Green
    } else {
        Write-Host "  ⚠ WinFSP 已安装但未运行 (状态: $($winfspService.Status))" -ForegroundColor Yellow
        Write-Host "    尝试启动服务..." -ForegroundColor Gray
        Start-Service -Name "WinFsp.Launcher" -ErrorAction SilentlyContinue
        if ($?) {
            Write-Host "  ✓ WinFSP 服务已启动" -ForegroundColor Green
        } else {
            Write-Host "  ✗ 无法启动 WinFSP 服务，请以管理员权限运行" -ForegroundColor Red
            $allOk = $false
        }
    }
} else {
    Write-Host "  ✗ WinFSP 未安装" -ForegroundColor Red
    Write-Host "    请从以下地址下载安装: https://github.com/winfsp/winfsp/releases" -ForegroundColor Gray
    $allOk = $false
}
Write-Host ""

# 检查 LLVM/Clang
Write-Host "[2/3] 检查 LLVM/Clang..." -ForegroundColor Yellow
$clangFound = $false

# 检查 clang 命令
$clangCmd = Get-Command clang -ErrorAction SilentlyContinue
if ($clangCmd) {
    Write-Host "  ✓ Clang 可用 (PATH)" -ForegroundColor Green
    $clangFound = $true
}

# 检查 LIBCLANG_PATH 环境变量
if (-not $clangFound) {
    if ($env:LIBCLANG_PATH) {
        $libclangPath = $env:LIBCLANG_PATH
        if (Test-Path "$libclangPath\clang.dll" -or Test-Path "$libclangPath\libclang.dll") {
            Write-Host "  ✓ LIBCLANG_PATH 已设置: $libclangPath" -ForegroundColor Green
            $clangFound = $true
        } else {
            Write-Host "  ⚠ LIBCLANG_PATH 已设置但无法找到 clang.dll: $libclangPath" -ForegroundColor Yellow
        }
    }
}

# 检查常见安装位置
if (-not $clangFound) {
    $commonPaths = @(
        "C:\Program Files\LLVM\bin",
        "C:\Program Files (x86)\LLVM\bin",
        "$env:ProgramFiles\LLVM\bin",
        "${env:ProgramFiles(x86)}\LLVM\bin"
    )
    
    foreach ($path in $commonPaths) {
        if (Test-Path "$path\clang.dll" -or Test-Path "$path\libclang.dll") {
            Write-Host "  ⚠ Clang 在以下位置找到但不在 PATH 中: $path" -ForegroundColor Yellow
            Write-Host "    运行以下命令设置环境变量:" -ForegroundColor Gray
            Write-Host "    `$env:LIBCLANG_PATH = `"$path`"" -ForegroundColor Cyan
            $clangFound = $true
            break
        }
    }
}

if (-not $clangFound) {
    Write-Host "  ✗ LLVM/Clang 未找到" -ForegroundColor Red
    Write-Host "    请选择以下方式之一安装:" -ForegroundColor Gray
    Write-Host "    1. 使用 winget: winget install LLVM.LLVM" -ForegroundColor Cyan
    Write-Host "    2. 手动下载: https://releases.llvm.org/" -ForegroundColor Cyan
    $allOk = $false
}
Write-Host ""

# 检查 Rust 工具链
Write-Host "[3/3] 检查 Rust 工具链..." -ForegroundColor Yellow
$rustCmd = Get-Command rustc -ErrorAction SilentlyContinue
if ($rustCmd) {
    $rustVersion = & rustc --version
    Write-Host "  ✓ Rust 工具链可用: $rustVersion" -ForegroundColor Green
} else {
    Write-Host "  ✗ Rust 未安装" -ForegroundColor Red
    Write-Host "    请访问 https://rustup.rs/ 安装" -ForegroundColor Gray
    $allOk = $false
}

$cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargoCmd) {
    $cargoVersion = & cargo --version
    Write-Host "  ✓ Cargo 可用: $cargoVersion" -ForegroundColor Green
} else {
    Write-Host "  ✗ Cargo 不可用" -ForegroundColor Red
    $allOk = $false
}
Write-Host ""

# 总结
Write-Host "========================" -ForegroundColor Cyan
if ($allOk) {
    Write-Host "✓ 所有依赖都已就绪！" -ForegroundColor Green
    Write-Host ""
    Write-Host "现在可以构建项目:" -ForegroundColor Cyan
    Write-Host "  cargo build --package musfuse-windows" -ForegroundColor White
    Write-Host ""
    Write-Host "或直接运行:" -ForegroundColor Cyan
    Write-Host "  cargo run --package musfuse-windows -- --source `"C:\Music`" --mount `"M:`"" -ForegroundColor White
} else {
    Write-Host "✗ 某些依赖缺失，请先安装缺失的组件" -ForegroundColor Red
    Write-Host ""
    Write-Host "详细说明请查看: crates\musfuse-windows\README.md" -ForegroundColor Gray
}
Write-Host ""
