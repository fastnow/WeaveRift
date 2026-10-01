# WeaveRift 构建指南（v1.0.5）

## 一、前置依赖

### 编译期依赖

| 依赖 | 用途 | 来源 |
|---|---|---|
| Rust 1.75+（MSVC target） | 编译 `WeaveRift.exe` / `jar_loader.dll` | [rust-lang.org](https://www.rust-lang.org/tools/install) |
| JDK 8 或更高 | 编译 `weaverift-agent.jar` | [Adoptium](https://adoptium.net/) |

> Agent 用 Java 8 语法编写（目标版本 1.12.2 跑在 JDK 8 上），JDK 17 也能编译，但需加 `--release 8` 或设置 `-source/-target 8`，否则游戏端 `UnsupportedClassVersionError`。

### Rust crate 依赖（`jar_loader`）

| crate | 版本 | 用途 |
|---|---|---|
| `windows` | 0.58 | Win32 API（GDI / OpenGL / LibraryLoader / Memory） |
| `jni` | 0.21 | JNI 调用 |
| `tiny_http` | 0.12 | 调试台 HTTP 服务 |

v2 起**不再需要**第三方 hooking crate —— `hook.rs` 自带 x64 inline hook 实现。

### 已移除的依赖

以下三个文件 v2 已**全部移除**，无需准备：

| 文件 | 移除原因 |
|---|---|
| `asm-9.7.jar` | 不再修改字节码 |
| `asm-commons-9.7.jar` | 同上 |
| `lwjgl-2.9.3.jar` | 不在 Java 层画 GL；打进 agent jar 会导致双份 LWJGL，`GLContext` 的 ThreadLocal 为空，所有 GL 调用静默 no-op |

### 运行时依赖（用户手动准备）

| 文件 | 来源 | 放置位置 |
|---|---|---|
| `obf2srg.srg` | [kettingpowered/MinecraftMappings](https://github.com/kettingpowered/MinecraftMappings) → `mappings` 分支 → 对应版本目录 | `release/WeaveRift/obf2srg.srg` |

**示例**：目标版本 1.12.2，下载 `1.12.2/obf2srg.srg`，放到 `release/WeaveRift/` 下。

> `mappings` 分支约 1.43 GB，只下载单个文件，不要克隆整个分支。
>
> **Forge 可以不提供**：运行时类名已是 SRG / MCP 名。原版必须提供。

## 二、本地一键构建

双击项目根目录的 `package.bat`：

1. 清理旧的 `release/`
2. `cargo build --release` → `WeaveRift.exe`
3. `jar_loader/` → `cargo build --release` → `jar_loader.dll`
4. `agent/` → `build.bat` 编译 Java → `weaverift-agent.jar`
5. 按目录结构放置产物

### 产物结构

```
release/
├── WeaveRift.exe
└── WeaveRift/
    ├── jar_loader.dll
    ├── weaverift-agent.jar
    ├── console.html          ← 可选，外置控制台（缺失则用内置兜底页面）
    └── obf2srg.srg           ← 用户手动放（Forge 可省略）
```

> `obf2srg.srg` 是运行时依赖，不会被编译脚本自动放进。
>
> `console.html` 放 DLL 同目录或 exe 同目录均可，改界面不用重新编译 DLL。

## 三、手动构建各模块

### 1. 主程序

```bat
cargo build --release
:: 产物: target/release/WeaveRift.exe
```

### 2. jar_loader.dll

```bat
cd jar_loader
cargo build --release
:: 产物: target/release/jar_loader.dll
```

> 不要用 `panic = "abort"`。cdylib 里 panic 越过 FFI 边界会直接崩进程，hook 体内已用 `catch_unwind` 兜底。

### 3. weaverift-agent.jar

```bat
cd agent
build.bat
:: 产物: build/weaverift-agent.jar
```

`build.bat` 做的事：

1. `javac` 编译 `src/main/java/**`（**无外部依赖**，不需要 classpath）
2. 打进 `MANIFEST.MF`
3. 打包成 jar

`MANIFEST.MF`：

```
Manifest-Version: 1.0
Agent-Class: com.fastnow.weaverift.WeaveRiftAgent
Can-Redefine-Classes: false
Can-Retransform-Classes: false
```

> `Can-Redefine-Classes` / `Can-Retransform-Classes` 必须显式设为 `false`。v2 不改字节码，声明这些能力会让部分反作弊多一个检测点。

等价命令：

```bat
javac -d out $(find src -name '*.java')
jar cfm build/weaverift-agent.jar MANIFEST.MF -C out .
```

## 四、GitHub Actions 自动发布

文件：`.github/workflows/auto-release.yml`

当提交信息包含 `[RELEASE]` 时，云端自动：

1. 读 `version.json`
2. 编译 `WeaveRift.exe` / `jar_loader.dll` / `weaverift-agent.jar`
3. 打包 `release.zip`
4. 创建 GitHub Release

触发方式：

```bat
git add version.json
git commit -m "[RELEASE] v1.0.5"
git push
```

> Actions 不会自动下载 `obf2srg.srg`。用户下载 Release 后需自行获取对应版本文件。

## 五、运行与验证

### 注入顺序

**先 agent，后 DLL。** DLL 的 `WAIT_AGENT` 阶段轮询 `RiftBridge`，agent 未就绪会持续重试。

### 验证清单

1. 日志出现 `[WeaveRift] agentmain` → agent 起来了
2. 日志出现 `SRG 载入: ... class=N field=M method=K`，N 在几千量级 → 映射正确
3. 日志出现 `wglSwapBuffers 已 hook (orig @ 0x...)` → hook 装上
4. 依次出现 `GL context 就绪` → `JavaVM 获取成功` → `GL 1.1 函数指针加载完成` → `wglShareLists 成功` → `独立 GL context 创建完成` → `HUD 字体初始化完成` → `agent 就绪，进入 Running`
5. 游戏画面出现红色边框 + 左上角 HUD

### 调试台

打开 `%TEMP%\WeaveRift\debug-url.txt`，里面的 URL 已带 token，直接用浏览器打开。

## 六、常见问题

| 现象 | 原因 | 解决 |
|---|---|---|
| `cargo not found` | Rust 未装或未加 PATH | 安装 Rust 并重开终端 |
| `javac not found` | JDK 未装或未加 PATH | 安装 JDK 并重开终端 |
| `程序包 org.objectweb.asm 不存在` | 残留旧源码 | v2 已移除 ASM，确认源码已升级到 v1.0.5 |
| `程序包 org.lwjgl.opengl 不存在` | 残留旧源码 | v2 已移除 LWJGL，确认源码已升级到 v1.0.5 |
| `UnsupportedClassVersionError` | 用高版本 JDK 编译未指定 target | 加 `--release 8` |
| 无 `[WeaveRift] agentmain` 输出 | attach 被禁 | JVM 参数加 `-Djdk.attach.allowAttachSelf=true`；确认无 `-XX:+DisableAttachMechanism` |
| `RiftBridge 不可见` | agent 未注入，或 `appendToSystemClassLoaderSearch` 失败 | 先注入 agent 再注入 DLL；检查日志 |
| 运行时 `字段缺失` | SRG 版本与游戏版本不符 | 下载对应版本的 `obf2srg.srg` |
| `orig @ 0x...` 是 32 位地址 | 编译成 32 位了 | 确认 target 是 `x86_64-pc-windows-msvc` |

## 七、v1 → v2 差异速查

| 项目 | v1.0.4 | v1.0.5 |
|---|---|---|
| 渲染时机 | JVMTI 注入 `bib.class`，插 `runGameLoop` | hook `wglSwapBuffers`（swap 之前） |
| 渲染层 | Java + LWJGL | native + opengl32.dll |
| GL context | 共用游戏的 | 独立 context + `wglShareLists` |
| 字节码修改 | ASM + `RedefineClasses` | 无 |
| JNI 线程 | 自建线程 + attach | 渲染线程 `GetEnv()` |
| `DllMain` | 建线程 | 只装 hook |
| 调试 | 两个日志文件 | `127.0.0.1` 调试台 + 反向控制 |
| 依赖 | asm / asm-commons / lwjgl / jvmti-bindings | windows / jni / tiny_http |
| `module_hide` | 有 | 移除 |