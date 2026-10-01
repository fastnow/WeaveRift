# WeaveRift

面向 **Windows + Minecraft（Java 版）** 的注入式调试框架。原生 DLL 注入 Minecraft 进程，hook `wglSwapBuffers` 拿到每帧渲染时机，在**独立 OpenGL context** 中绘制游戏内 HUD，并通过 `127.0.0.1` 调试台输出实时日志与反向控制。

支持原版 / Forge。

> **声明**：本工具仅限学习、调试、研究等合法场景。请只对**你自己拥有或有权操作**的进程使用；未经授权注入他人程序属违法行为，由使用者自行承担全部责任。
>
> **只在单机存档或你自己的自建服务器中使用。** 在带反作弊的公共服务器注入会触发封号。

---

## 架构

```
WeaveRift.exe
  ↓ 注入
jar_loader.dll (Rust)
  ├─ hook opengl32!wglSwapBuffers
  │    ├─ 帧驱动状态机（每帧 8ms 预算）
  │    ├─ 切到独立 GL context（wglShareLists 与游戏共享）
  │    ├─ JNI GetEnv() 读快照（不 attach 线程）
  │    ├─ 绘制 HUD
  │    ├─ 还原游戏 context
  │    └─ 调原始 wglSwapBuffers
  ├─ HTTP 调试台 127.0.0.1:11451
  └─ 日志双写（内存环形缓冲 + %TEMP% 文件）
      ↓ JNI
weaverift-agent.jar (Java)
  └─ RiftBridge.snapshotArray()
```

**为什么不改字节码了**：v1 通过 JVMTI `RedefineClasses` 把渲染代码插进 `bib.class`，需要维护 ASM、处理栈帧、JIT 内联、插入点时序（画在 `Display.update()` 之后会被下一帧 `glClear` 抹掉）。v2 在 native 层 hook swap，时序天然正确，游戏代码一行不动。

## 核心特性

- **零字节码修改**：不依赖 ASM / `Instrumentation` / `RedefineClasses`，纯只读采样
- **画在 swap 之前**：不存在"画进下一帧 back buffer 被清掉"的问题
- **独立 GL context**：`wglShareLists` 与游戏共享资源，GL 状态互不污染
- **JNI 只用 `GetEnv()`**：渲染线程天然 attached，不触发 `ThreadStart` 事件
- **帧驱动状态机**：所有初始化切片推进，绝不在 `DllMain` 建线程
- **127.0.0.1 调试台**：实时日志流、状态自检、反向控制（开关 HUD / 调采样率 / 重扫映射 / 卸载 hook）
- **DllMain 不建线程**：避免 loader lock 死锁
- **跨版本**：换 SRG 映射文件即可适配不同 Minecraft 版本
- **跨加载器**：原版 / Forge 共用同一套 SRG 映射

## 快速上手

### 注入顺序（重要）

**先注入 agent，再注入 DLL。** DLL 的 `WAIT_AGENT` 阶段会轮询 `RiftBridge`，agent 没就绪会一直重试。

1. 启动 Minecraft，进入世界
2. 以管理员身份运行 `WeaveRift.exe`
3. 选 `🎯 Select Target Process` → 选中 `javaw.exe`
4. 选 `📦 Select Agent JAR` → `release/WeaveRift/weaverift-agent.jar`，**注入**
5. 再选 `🚀 Inject Now` 注入 `jar_loader.dll`
6. 打开 `%TEMP%\WeaveRift\debug-url.txt` 里的 URL（带 token），浏览器打开

> 也支持 `⏳ Auto-Wait & Inject`：选它之后**再启动 Minecraft**，会自动等待进程出现。但 agent 仍需先于 DLL 注入。

### 首次成功的标志

游戏画面四周出现红色边框，左上角出现 HUD 面板：

```
WeaveRift  f123
hook  60 Hz      ← 0 就是 hook 没装上
jni   ok         ← NO VM：jvm.dll 没找到 / 注错进程
ctx   ok         ← NO CTX：wglCreateContext 失败
state Running
xyz   1614.5 4.0 -557.4
rot   137.2 / -12.4
hp    20/20   ents 48
```

前三行是自检行，任何一行红了立刻知道断在哪一层。

## 用户需要手动准备的依赖

| 依赖 | 来源 | 放置位置 | 用途 |
|---|---|---|---|
| `obf2srg.srg` | [kettingpowered/MinecraftMappings](https://github.com/kettingpowered/MinecraftMappings) `mappings` 分支 → 对应版本目录 | `release/WeaveRift/obf2srg.srg` | SRG 映射表，查混淆名 |

> **提示**：`mappings` 分支体积约 1.43 GB，请只下载对应版本目录下的单个 `obf2srg.srg`，不要克隆整个分支。
>
> **Forge 环境可以不给**：Forge 运行时类名已经是 SRG / MCP 名，留空即可。原版**必须**提供。

v2 起**不再需要** `asm-9.7.jar` / `asm-commons-9.7.jar` / `lwjgl-2.9.3.jar` —— 不改字节码、不在 Java 层画 GL，这三个依赖已全部移除。

## SRG 映射文件说明

SRG（Searge）是 Forge 生态的标准中间名。原版运行时使用混淆名（1.12.2 的 `net.minecraft.client.Minecraft` 实际是 `bib`），反射必须查表换成真实名字。

本项目只支持原版 + Forge，不需要 Intermediary（Fabric），只需 `obf2srg.srg` 一个文件。

| 文件 | 用途 |
|---|---|
| `obf2srg.srg` | 混淆名 ↔ SRG 名，Agent 用它查每个字段 / 方法的真实名字 |

不需要 `obf2mcp.srg` / `srg2mcp.srg` / `obf2spigot.srg` —— 那些是给开发工具和 Spigot 服务端用的。

## 调试台

浏览器打开 `debug-url.txt` 里的链接（token 每次启动随机生成）。

| 端点 | 用途 |
|---|---|
| `/` | 控制台页面（同目录有 `console.html` 会优先用，改界面不用重新编译） |
| `/api/status` | 状态机 / HUD 开关 / 采样率 / VM / context |
| `/api/logs?since=` | 增量日志流 |
| `/api/command` | 反向控制 |

可用命令：

| 命令 | 作用 |
|---|---|
| `hud_on` / `hud_off` | 开关 HUD 绘制 |
| `hz=N` | 调整采样率（1–120，默认 20） |
| `rescan` | 让 Java 重新加载 SRG 映射 |
| `status` | 查看 Java 侧状态 |
| `error` | 查看 Java 侧最近错误 |
| `unhook` | 卸载 hook |

**安全设计**（本地 HTTP 服务不是"本地所以安全"）：

- 只绑 `127.0.0.1`，改 `0.0.0.0` 会把游戏状态暴露到局域网
- 每次启动随机 token，缺失或非法一律 403
- 校验 `Host` 头，拒绝非本地来源 —— 防 DNS rebinding
- CORS 只放行 `http://127.0.0.1:11451`，不设 `*`

## 注意事项

- **必须以管理员身份运行** `WeaveRift.exe`
- **JVM 必须允许 self-attach**：启动参数加 `-Djdk.attach.allowAttachSelf=true`（PCL / HMCL 等第三方启动器可在 JVM 参数栏填写，官方启动器加不了）
- 若启动器加了 `-XX:+DisableAttachMechanism`，attach 会直接失败，agent 注入不进去
- **杀软误报**：会被标记为注入行为，请自行判断来源并加入信任区
- **反作弊**：**只在单机、自建服务器或你自己的测试环境使用**
- 日志在 `%TEMP%\WeaveRift\weaverift.log`；Java 侧日志进 Minecraft 的 `latest.log`

## 排错

| 现象 | 原因 | 解决 |
|---|---|---|
| 日志无 `wglSwapBuffers 已 hook` | 注入太早 / 注错进程 | 确认注入的是 `javaw.exe`；状态机会重试，通常自愈 |
| `hook 0 Hz` | hook 未生效或游戏未渲染 | 确认已进入世界（主菜单不渲染） |
| `jni NO VM` | `jvm.dll` 未加载 / 注错进程 | 确认目标进程是 JVM |
| `ctx NO CTX` | `wglCreateContext` 失败 | 看日志是否反复出现 `HDC 变化` |
| `no snapshot (agent 未就绪?)` | agent 未注入 / `RiftBridge` 不可见 | 先注入 agent；确认 `appendToSystemClassLoaderSearch` 成功 |
| Java 侧报 `字段缺失` | SRG 版本与游戏版本不符 | 下载对应版本的 `obf2srg.srg` |
| 日志 `本帧耗时 xxxus` | 单帧超 8ms 预算 | 调低采样 Hz |
| 画面闪 / 掉帧 | 采样过密 | 控制台滑杆降到 5–10 Hz |

## 已知限制

- hook 只覆盖 14 字节，假设目标函数开头无 rip 相对寻址。`wglSwapBuffers` 满足；hook 其他函数请换成熟 hooking 库
- **高版本 MC（1.13+）走 LWJGL3 / GLFW，最终调 `gdi32!SwapBuffers` 而非 `wglSwapBuffers`**。跨版本时改 hook 目标即可
- HUD 文字只支持 ASCII（GDI 烘焙 96 个显示列表）
- 真全屏模式下 HDC 可能变化，代码已处理重建，但未在所有环境实测

## 更新日志

### v1.0.5（Native Hook 版本）

**架构重写**，从「JVMTI 改字节码 + Java 层 GL 渲染」切到「native hook + 独立 GL context + 127.0.0.1 调试台」。

- **核心变更**：hook `opengl32!wglSwapBuffers` 拿渲染时机，渲染完全绕过 JVM
- **独立 GL context**：用 hook 传入的游戏 HDC 建第二个 context，`wglShareLists` 共享资源，GL 状态与游戏隔离
- **JNI 只用 `GetEnv()`**：渲染线程天然 attached，不调用 `AttachCurrentThread`，避免触发 `ThreadStart` 事件
- **帧驱动状态机**：`DllMain` 只装 hook，初始化全部切片推进，杜绝 loader lock 死锁
- **新增** `127.0.0.1` 调试台：日志流、状态自检、反向控制（开关 HUD / 调采样率 / 重扫映射 / 卸载 hook）
- **新增** `ClassLoaderUtil`：从 `Launch.classLoader` 或 `getAllLoadedClasses()` 定位游戏 ClassLoader
- **新增** `console.html`：外置控制台页面，改界面无需重新编译
- **移除** ASM 依赖：不再改字节码，`asm` / `asm-commons` 从 `agent/libs/` 删除
- **移除** `lwjgl-2.9.3.jar`：不在 Java 层画 GL，避免双份 LWJGL 导致 `GLContext` ThreadLocal 为空
- **移除** `RiftRender.java` / `RiftTransformer.java` / `NativeBridge.redefineClass` / `jvmti-bindings`
- **移除** `module_hide`：研究 / 调试工具不需要 PEB 断链，且会挡住自己的调试器
- 自包含 x64 inline hook，不再依赖第三方 hooking crate
- Agent 改为只读采样，20Hz 写入 `RiftBridge` 快照

### v1.0.4

- 改用 JVMTI `RedefineClasses` 替换已加载的类
- 新增 `RiftRender.java` / `NativeBridge.java`
- 支持原版 + Forge，放弃 Fabric

## 许可

Apache-2.0 | Copyright © 2026 FastNow Studio