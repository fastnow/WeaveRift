# WeaveRift 构建指南

## 一、前置依赖

### 编译期依赖

| 依赖 | 用途 | 来源 |
|---|---|---|
| Rust 1.75+ | 编译 WeaveRift.exe / jar_loader.dll / module_hide | [rust-lang.org](https://www.rust-lang.org/tools/install) |
| JDK 17 或更高 | 编译 weaverift-agent.jar | [Adoptium](https://adoptium.net/) 或本地已有 JDK |

### Agent 编译所需的 JAR（已自带）

以下三个文件已经放在 `agent/libs/`，**无需手动下载**：

| 文件 | 用途 |
|---|---|
| `asm-9.7.jar` | 字节码修改 |
| `asm-commons-9.7.jar` | ASM 工具类 |
| `lwjgl-2.9.3.jar` | 编译期链接 OpenGL（运行时由游戏自带） |

> **注意**：如果你是从 GitHub Release 下载的预编译版本，那 `agent/libs/` 不会包含这三个文件。**只有从源码编译才需要它们**。

### 运行时依赖（用户手动准备）

| 文件 | 来源 | 放置位置 |
|---|---|---|
| `obf2srg.srg` | [kettingpowered/MinecraftMappings](https://github.com/kettingpowered/MinecraftMappings) 仓库 → `mappings` 分支 → 对应版本目录 → `obf2srg.srg` | `release/WeaveRift/obf2srg.srg` |

**示例**：你的目标版本是 1.12.2，就去下载 `1.12.2/obf2srg.srg`，放到 `release/WeaveRift/` 下。

> **提示**：`mappings` 分支体积很大（约 1.43 GB），请只下载对应版本目录下的单个 `obf2srg.srg` 文件，不要克隆整个分支。

## 二、本地一键构建

双击运行项目根目录的 `package.bat`，它会自动完成：

1. 清理旧的 `release/` 目录
2. `cargo build --release` → 生成 `WeaveRift.exe`
3. 进入 `jar_loader/` → `cargo build --release` → 生成 `jar_loader.dll`
4. 进入 `agent/` → 调 `build.bat` 编译 Java → 生成 `weaverift-agent-1.0.0.jar`
5. 把产物按正确目录结构放到 `release/`

### 产物结构

release/
├── WeaveRift.exe
└── WeaveRift/
    ├── jar_loader.dll
    └── weaverift-agent-1.0.0.jar

> **别忘了**：编译完成后，把 `obf2srg.srg` 手动复制到 `release/WeaveRift/` 下。这是运行时依赖，不会被编译脚本自动放进。

## 三、手动构建各模块

### 1. 主程序 WeaveRift.exe

cd WeaveRift
cargo build --release
:: 产物: target/release/WeaveRift.exe

### 2. jar_loader.dll

cd jar_loader
cargo build --release
:: 产物: target/release/jar_loader.dll

### 3. weaverift-agent.jar

cd agent
build.bat
:: 产物: build/weaverift-agent-1.0.0.jar

`build.bat` 做的事：

1. 编译 Java 源文件（含 ASM / LWJGL 依赖）
2. 解压 ASM 到临时目录
3. 把 class 文件和 ASM 一起打包进 JAR
4. 生成 `build/weaverift-agent-1.0.0.jar`

## 四、GitHub Actions 自动发布

文件：`.github/workflows/auto-release.yml`

当最新提交信息包含 `[RELEASE]` 时，云端自动：

1. 读 `version.json`
2. 编译 `WeaveRift.exe` / `jar_loader.dll` / `weaverift-agent.jar`
3. 打包成 `release.zip`
4. 创建 GitHub Release

### 触发方式

git add version.json
git commit -m "[RELEASE] v1.0.4"
git push

> **GitHub Actions 不会自动下载 `obf2srg.srg`**。用户下载 Release 后，需要自己去 kettingpowered 仓库下载对应版本的 `obf2srg.srg`，放到 `release/WeaveRift/` 下。

## 五、常见问题

| 现象 | 原因 | 解决 |
|---|---|---|
| `cargo not found` | Rust 没装或没加 PATH | 安装 Rust 并重开终端 |
| `javac not found` | JDK 没装或没加 PATH | 安装 JDK 17+ |
| `程序包 org.objectweb.asm 不存在` | ASM JAR 没放对位置 | 确认 `agent/libs/asm-9.7.jar` 存在 |
| `程序包 org.lwjgl.opengl 不存在` | LWJGL JAR 缺失 | 确认 `agent/libs/lwjgl-2.9.3.jar` 存在 |
| 运行时 `SRG path not set` | 未设置系统属性 | 检查 `jar_loader` 是否成功设了 `weaverift.srg` |
| 运行时 `Class not found: bib` | SRG 文件版本不匹配 | 下载与目标 Minecraft 版本对应的 `obf2srg.srg` |
