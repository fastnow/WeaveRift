# WeaveRift

面向 **Windows + Minecraft（Java 版）** 的注入式客户端。

> **声明**：本工具仅限学习、调试、研究等合法场景。请只对**你自己拥有或有权操作**的进程使用；未经授权注入他人程序属违法行为，由使用者自行承担全部责任。
>

---

## 功能

- **游戏内 HUD**：玩家坐标 / 朝向 / 血量 / 实体数 / 帧率
- **Tracers**：从屏幕底部指向实体（玩家红 / 怪物橙 / 动物绿）
- **ESP**：实体 3D 包围盒
- **Right Shift**：显示 / 隐藏 HUD 与客户端面板
- **模块系统**：功能以模块形式注册，可扩展

---

## 快速上手

### 1. 前置依赖

| 依赖 | 用途 |
|---|---|
| **JDK 8 或更高** | 运行 agent |
| **`obf2srg.srg`** | SRG 映射表（见下方） |

**下载 `obf2srg.srg`**：

从 [kettingpowered/MinecraftMappings](https://github.com/kettingpowered/MinecraftMappings) 的 `mappings` 分支，下载**对应版本目录**下的 `obf2srg.srg`（如 `1.12.2/obf2srg.srg`），放到 `release/WeaveRift/` 下。

> `mappings` 分支约 1.43 GB，**只下载单个文件**，不要克隆整个分支。
>
> **Forge 环境可以不给**：Forge 运行时类名已是 SRG 名，留空即可。原版必须提供。

### 2. 解压 release

```text
release/
├── WeaveRift.exe
└── WeaveRift/
    ├── jar_loader.dll
    ├── client.dll
    ├── weaverift-agent.jar
    ├── obf2srg.srg      ← 手动放
    └── console.html     ← 可选
```

### 3. 启动游戏

用 PCL / HMCL 等启动器启动 Minecraft。**JVM 参数加**：

```text
-Djdk.attach.allowAttachSelf=true
```

> 官方启动器加不了这条参数，建议用第三方启动器。

### 4. 注入

**以管理员身份运行 `WeaveRift.exe`**：

1. 选 `🎯 Select Target Process` → 选中 `javaw.exe`
2. 选 `📦 Select Agent JAR` → 指向 `weaverift-agent.jar`
3. 再选 `🚀 Inject Now` → 注入 `jar_loader.dll`
4. 进游戏世界，画面四周出现红色边框 = 成功

### 5. 调试台

`%TEMP%\WeaveRift\debug-url.txt` 里有带 token 的 URL，浏览器打开可看日志、调参数。

---

## 操作

| 按键 | 功能 |
|---|---|
| **Right Shift** | 显示 / 隐藏 HUD 与客户端面板 |

**客户端面板**：显示已注册模块，可勾选开关。

---

## 排错

| 现象 | 原因 | 解决 |
|---|---|---|
| 无 `wglSwapBuffers 已 hook` | 注入太早 / 注错进程 | 确认注入 `javaw.exe` |
| `hook 0 Hz` | 未进入世界 | 主菜单不渲染，进世界后自愈 |
| `jni NO VM` | `jvm.dll` 未加载 | 确认目标进程是 JVM |
| `no snapshot` | agent 未注入 | 先注 agent 再注 DLL |
| `字段缺失` | SRG 版本不符 | 下载对应版本的 `obf2srg.srg` |
| `本帧耗时 xxxus` | 单帧超预算 | 调低采样 Hz |
| 画面闪 / 掉帧 | 采样过密 | 调试台调低采样率 |

---

## 注意事项

- **杀软误报**：会被标记为注入行为，请自行判断来源并加入信任区
- **反作弊**：只在单机、自建服务器或你自己的测试环境使用
- 日志在 `%TEMP%\WeaveRift\weaverift.log`
- Java 侧日志进 Minecraft 的 `latest.log`

---

## 常见问题

**Q: 官方启动器能用吗？**

A: 全部启动器均可使用，推荐使用**用 PCL / HMCL 等第三方启动器**。

**Q: Forge 需要用吗？**

A: 支持。Forge 环境类名已是 SRG 名，**不需要额外提供 `obf2srg.srg`**。

**Q: 支持哪些版本？**

A: **当前版本为 1.12.2 原版 + Forge**。跨版本需要替换 `obf2srg.srg` 并适配渲染层，详见发布日志。

---

## 从源码构建

### 依赖

| 依赖 | 版本 | 用途 |
|---|---|---|
| **Rust** | 1.75+（MSVC target） | 编译 `WeaveRift.exe` / `jar_loader.dll` / `client.dll` |
| **JDK** | 8+ | 编译 `weaverift-agent.jar` |

### 一键构建

双击项目根目录 `package.bat`即可。

### 目录结构

```text
WeaveRift/
├── src/            # WeaveRift.exe（注入器）
├── jar_loader/     # jar_loader.dll（native 核心）
├── client/         # client.dll（模块系统）
├── agent/          # weaverift-agent.jar（Java 采样）
├── console.html    # 调试台页面（可选）
└── release/        # 构建产物
```

### 自动发布

推送到 `main` 且 commit message 含 `[RELEASE]` 时，GitHub Actions 自动编译并发布。

---

## 许可

Apache-2.0 | Copyright © 2026 FastNow Studio