# WeaveRift

面向 **Windows + Minecraft（Java 版）** 的注入框架，支持原版 / Forge / Fabric，通过 JVMTI 在运行时修改已加载的类字节码，配合 OpenGL 实现游戏内渲染。

> **声明**：本工具仅限学习、调试、研究等合法场景。请只对**你自己拥有或有权操作**的进程使用；未经授权注入他人程序属违法行为，由使用者自行承担全部责任。

## 核心特性

- **零启动参数**：不需要修改 Minecraft 的 JVM 启动参数
- **运行时字节码修改**：通过 JVMTI `RedefineClasses` 替换已加载的类，不受 `Instrumentation` Attach 模式限制
- **跨版本**：换 SRG 映射文件即可适配不同 Minecraft 版本
- **跨加载器**：原版 / Forge / Fabric 共用同一套 SRG 映射
- **OpenGL 渲染**：直接画在游戏画布上，不是外挂窗口
- **模块隐藏**：被注入 DLL 从 PEB 三条加载链摘除（`module_hide`）
- **可扩展**：社区可往 `RiftRender` 里加任意 UI / ESP / HUD 模块

## 架构概览

WeaveRift.exe (Rust)
    │ 1. 写共享内存：JAR 路径
    │ 2. LoadLibraryW 注入 jar_loader.dll
    ▼
jar_loader.dll (Rust)
    │ 3. 读共享内存，从 JAR 路径推导 SRG 路径
    │ 4. 设置系统属性 weaverift.srg / weaverift.dll
    │ 5. Attach API 加载 weaverift-agent.jar
    ▼
weaverift-agent.jar (Java)
    │ 6. 解析 SRG 映射表
    │ 7. 反射读游戏数据
    │ 8. 读 bib.class 原始字节码 → ASM 修改
    │ 9. 调 NativeBridge.redefineClass() → JNI
    ▼
jar_loader.dll 的 redefineClass
    │ 10. JVMTI RedefineClasses 替换已加载的类
    ▼
游戏 render 方法末尾插入 RiftRender.onRender()
    ▼
RiftRender 用 OpenGL 在游戏画布上绘制

## 快速上手

### 方式一：自动等待（推荐）

1. 以管理员身份运行 `WeaveRift.exe`
2. 选 `📦 Select Agent JAR` → 选 `release/WeaveRift/weaverift-agent-1.0.0.jar`
3. 选 `⏳ Auto-Wait & Inject`
4. **看到 "Waiting for Minecraft..." 后再启动 Minecraft**

### 方式二：先启动游戏

1. 先启动 Minecraft，进到主菜单
2. 运行 `WeaveRift.exe`，选 `🎯 Select Target Process` 选中 `javaw.exe`
3. 选 Agent JAR，然后选 `🚀 Inject Now`

## 目录结构

WeaveRift/
├── src/                      # 主程序（Rust）
│   ├── main.rs               # 交互菜单 + CLI 入口
│   ├── ipc.rs                # 共享内存（传 JAR 路径）
│   ├── process_finder.rs     # 进程扫描
│   ├── logger.rs             # 日志
│   └── injector/             # DLL 注入实现
├── jar_loader/               # jar_loader.dll（Rust + JVMTI）
│   ├── src/lib.rs
│   └── Cargo.toml
├── agent/                    # weaverift-agent.jar（Java）
│   ├── src/main/java/com/fastnow/weaverift/
│   │   ├── WeaveRiftAgent.java   # agentmain 入口
│   │   ├── NativeBridge.java     # JNI 声明
│   │   └── render/RiftRender.java # OpenGL 渲染
│   ├── libs/                 # 编译依赖 JAR
│   │   ├── asm-9.7.jar
│   │   ├── asm-commons-9.7.jar
│   │   └── lwjgl-2.9.3.jar
│   ├── build.bat
│   └── manifest.txt
├── module_hide/              # PEB 断链（Rust lib）
├── release/                  # 打包输出
│   ├── WeaveRift.exe
│   └── WeaveRift/
│       ├── jar_loader.dll
│       ├── weaverift-agent-1.0.0.jar
│       └── obf2srg.srg       # SRG 映射文件（需要手动下载）
├── package.bat
└── Cargo.toml

## 用户需要手动准备的依赖

| 依赖 | 来源 | 放置位置 | 用途 |
|---|---|---|---|
| `obf2srg.srg` | 从 [kettingpowered/MinecraftMappings](https://github.com/kettingpowered/MinecraftMappings) 的 `mappings` 分支下载对应版本的 `obf2srg.srg` | `release/WeaveRift/obf2srg.srg` | SRG 映射表，用于查混淆名 |
| `asm-9.7.jar` | 本项目 `agent/libs/` 目录自带 | — | 字节码修改 |
| `asm-commons-9.7.jar` | 本项目 `agent/libs/` 目录自带 | — | 字节码修改（工具类） |
| `lwjgl-2.9.3.jar` | 本项目 `agent/libs/` 目录自带 | — | 编译期链接 OpenGL（运行时由游戏自带） |

> **为什么 `asm` 和 `lwjgl` 已经在 `agent/libs/` 里？**
> 为了让用户可以离线编译 Agent。`build.bat` 会自动把 ASM 打进 Agent JAR，运行时不再依赖外部 ASM。

> **提示**：`mappings` 分支体积很大（约 1.43 GB），请只下载对应版本目录下的单个 `obf2srg.srg` 文件，不要克隆整个分支。

## SRG 映射文件说明

SRG（Searge）是 Forge 生态的标准中间名。游戏运行时使用 SRG 名而不是 MCP 名，所以反射代码必须用 SRG 名对应的混淆名。

本项目**只支持原版 + Forge**，不需要 Intermediary（Fabric）。所以只需要下载 `obf2srg.srg` 一个文件。

| 文件 | 用途 |
|---|---|
| `obf2srg.srg` | 混淆名 ↔ SRG 名，Agent 用它查每个字段/方法的真实名字 |

不需要下载 `obf2mcp.srg`、`srg2mcp.srg`、`obf2spigot.srg` 等文件——那些是给开发工具和 Spigot 服务端用的，与运行时反射无关。

## 注意事项

- **必须以管理员身份运行** `WeaveRift.exe`
- **JVM 必须允许 self-attach**：启动参数加 `-Djdk.attach.allowAttachSelf=true`
- **杀软误报**：注入 / JVMTI 操作会被杀软标记，请自行判断来源并加入信任区
- **反作弊**：在带反作弊的服务器注入属作弊，会触发封号。**只在单机、自建服务器或你自己的测试环境使用**
- 日志在 `%TEMP%\WeaveRift.log`，Agent 日志在 Minecraft 的 `latest.log`

## 更新日志

### v1.0.4（JVMTI 渲染闭环版本）

- **核心突破**：改用 JVMTI `RedefineClasses` 替换已加载的类，彻底解决 Attach 模式无法 retransform 的问题
- 新增 `agent/render/RiftRender.java`：OpenGL 渲染入口
- 新增 `agent/NativeBridge.java`：JNI 桥接，Java 调 JVMTI
- 新增 `jar_loader/src/lib.rs` 中的 `Java_com_fastnow_weaverift_NativeBridge_redefineClass` 导出
- 新增 `jvmti-bindings` 依赖，用于调用 JVMTI API
- 移除 `RiftTransformer.java`（不再需要 `ClassFileTransformer`）
- Agent 启动时自动从 JAR 路径推导 SRG / DLL 路径，无需硬编码
- 支持原版 + Forge，放弃 Fabric（避免 Intermediary 双映射维护）

## 许可

Apache-2.0 | Copyright © 2026 FastNow Studio