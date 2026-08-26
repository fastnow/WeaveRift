# FlashDllInjector：下一代 Minecraft Dll 注入器

一个面向 **Windows + Minecraft（Java 版）** 的高性能 **DLL 注入工具**，采用命令行交互菜单。

小白也能直接使用：启动游戏 → 运行工具 → 选中进程 → 选中 DLL → 注入，就这么简单。

> **重要声明**：本工具仅用于**学习、调试、研究**等合法用途，请只对你**自己拥有或有权操作**的进程使用。未经允许向他人程序注入属于违法行为，后果自负。

---

## 它能做什么？

- 自动扫描当前正在运行的 **Minecraft（Java 进程 javaw / java）**
- 自动过滤掉浏览器等无关进程（不会把 Chrome / Edge 当作目标）
- 把你自己写的 **DLL 文件** 注入到选中的游戏进程里
- 内置 3 种注入方式，按需选择
- 附带一个可选的「**FDI 开场动画覆盖层**」（`core.dll`），注入后会在游戏窗口上播放一段类似电影厂牌的开场动画

---

## 功能特性

| 特性 | 说明 |
|------|------|
| 智能进程识别 | 只显示 Java 进程，自动排除浏览器等无关进程 |
| 三种注入方式 | `LoadLibraryW` / `Reflective` / `ManualMap` |
| 开场动画覆盖层 | 注入 `fdi/core.dll` 可在游戏画面播放 "FDI" 电影式片头 |
| 交互式菜单 | 中文 + 图标，全程键盘选择即可操作 |
| 自动日志 | 运行日志写入 `%TEMP%\FlashDllInjector.log` |
| 自动判断位数 | 注入前检测目标进程是 32 位还是 64 位 |

---

## 技术信息

- 语言：**Rust**（2021 edition）
- 目标平台：**Windows**
- 授权协议：**Apache-2.0**
- 当前版本：**v1.0.2**

### 依赖库（编译时自动下载）
`windows` · `sysinfo` · `rfd` · `inquire` · `anyhow` · `thiserror` · `chrono` · `winres`

---

## 目录结构

```
FlashDllInjector/
├── src/
│   ├── main.rs           # 主程序（交互菜单）
│   ├── lib.rs            # 库入口
│   ├── process_finder.rs # 进程查找 / 过滤
│   ├── injector/         # 三种注入方式的实现
│   │   ├── loadlibrary.rs  # LoadLibraryW 注入
│   │   ├── reflective.rs   # Reflective 反射注入
│   │   ├── manualmap.rs    # ManualMap 手动映射
│   │   ├── pe_util.rs      # PE 文件解析
│   │   └── mod.rs          # 注入入口与公共类型
│   ├── logger.rs         # 日志
│   └── error.rs          # 错误类型
├── fdi/                  # 覆盖层 core.dll（电影开场动画）
├── jar_loader/           # JNI 相关辅助库（可选）
├── module_hide/          # PEB 模块隐藏（被注入 DLL 自隐藏）
├── package.bat           # 一键编译打包脚本
├── build.rs              # 编译时写入图标/版本信息
├── icon.ico              # 程序图标
└── Cargo.toml
```

---

## 如何获取

### 方式一：直接使用打包好的版本（推荐）
1. 让开发者/发布方给你 **release** 文件夹，里面应该有：
   - `FlashDllInjector.exe`（主程序）
   - `icon.ico`（图标，可选）
   - `fdi/core.dll`（覆盖层动画，可选，没有也不影响注入）
2. **把整个文件夹放在一起**，不要单独挪动 exe，否则找不到 `fdi/core.dll`。
3. 跳转到下方「🚀 小白使用教程」。

### 方式二：自己编译
需要先安装：
- **[Rust](https://www.rust-lang.org/tools/install)**（必须）
- **Windows 10 / 11**（必须是 Windows，本工具不跨平台）

**一键编译打包**：直接双击 `package.bat`，它会自动：
1. 清理旧编译产物
2. 编译 Release 版主程序
3. 编译 `fdi/core.dll` 覆盖层
4. 把所有文件放到 **`release/`** 文件夹里

完成后打开 `release` 文件夹即可使用。

> 也可以手动编译主程序：
> ```
> cargo build --release
> ```
> 生成的文件在 `target\release\FlashDllInjector.exe`。

---

## 草履虫都看得懂的使用教程

### 第 1 步：准备 DLL
请准备一个**你自己编译的 DLL 文件**（必须是为 Windows 编译的 `.dll`）。
- 用什么语言写都可以，只要能生成 Windows DLL 即可（C/C++、Rust 等）。
- 打包好的 `fdi/core.dll` 是覆盖层动画，不是必须的；如果你只是测试，可以直接注入它看效果。

### 第 2 步：以管理员身份运行工具
**重要！** 找到 `FlashDllInjector.exe`，**右键 → 以管理员身份运行**。
> 因为要向别的进程写内存，必须拥有管理员权限，否则会报「OpenProcess 失败 / 权限不足」。

### 第 3 步：先启动 Minecraft
先正常打开你的 Minecraft（Java 版）游戏，让游戏进程运行起来，然后再回到这个工具窗口。

### 第 4 步：选择目标进程
在菜单里选择 **`🎯 Select Target Process`**：
- 如果只找到一个 Java 进程 → 会自动选中。
- 如果有多个 → 会列出，用方向键选好按回车。
- 工具会自动过滤掉浏览器等无关进程，你只需要找 `javaw.exe` 或带游戏标题的那一项。

### 第 5 步：选择注入方式
选择 **`💉 Select Inject Method`**，三种方式的区别见下表：

| 方式 | 说明 | 稳定性 | 隐蔽性 | 适用 |
|------|------|:------:|:------:|------|
| `LoadLibraryW` | 最标准、最简单的注入方式（推荐） | ⭐⭐⭐ | ⭐ | 日常使用，最不容易出错 |
| `Reflective` | DLL 自己把自己映射进内存，不需要 DLL 落盘 | ⭐⭐ | ⭐⭐ | DLL 导出了 `ReflectiveLoader` 函数时 |
| `ManualMap` | 注入器手动把所有 PE 段写进内存 | ⭐ | ⭐⭐⭐ | 追求隐蔽、不想在目标进程里留下 LoadLibrary 痕迹时 |

> 小白请优先选 **`LoadLibraryW — Simple & Stable`**。

### 第 6 步：添加 DLL
选择 **`📁 Add DLL(s)`**，在弹出的文件窗口里选中你的 DLL 文件。
- 可以一次选多个。
- 用 **`🗑 Remove DLL`** 删掉某个，用 **`🧹 Clear DLL List`** 清空。

### 第 7 步：执行注入
选择 **`🚀 Execute Injection`**，工具会先显示目标位数（64/32），然后询问 **Execute?**，回车确认。
- 如果一切正常，会看到类似 `✅ All injected successfully.` 的提示。
- 如果 `fdi/core.dll` 存在，会先自动注入覆盖层动画。

### 第 8 步：退出
选择 **`👋 Exit`** 退出程序。

---

## 常见问题与注意事项

### 权限问题
- **必须以管理员身份运行**。普通权限下会报 `OpenProcess` 失败或权限不足。
- 如果游戏是用管理员权限启动的，工具也必须是管理员，否则无法注入。

### 位数匹配（很关键）
- **64 位进程 → 必须注入 64 位 DLL**
- **32 位进程 → 必须注入 32 位 DLL**
- 位数不匹配会失败（LoadLibraryW 报 `arch mismatch`；Reflective 报 `Not 64-bit`）。
- 游戏大多是 64 位，如果你的 DLL 是 32 位编译的，要重新编译成 64 位。

### 杀毒软件误报
- DLL 注入、手动映射、反检测这类操作会被**几乎所有杀毒软件标记**为风险程序。
- 如果被杀毒软件拦截或误删，请**把它加入白名单 / 信任区**。这通常是误报，但仍请自行判断来源是否可靠。

### 日志在哪里
- 日志自动保存在 `%TEMP%\FlashDllInjector.log`。
- 报错时打开这个文件，把内容发给开发者更便于排查。
- `%TEMP%` 通常在 `C:\Users\你的用户名\AppData\Local\Temp`。

### 反作弊 / 反作弊服务器
- 在带**反作弊系统**的服务器（如 Hypixel 等大型服务器）或某些平台环境中，注入 DLL 属于作弊行为，**会触发封号**。
- **请只在单机、自建服务器或你自己的测试环境中使用**。


### 三种注入方式的坑
- **Reflective**：要求你的 DLL 里有一个导出函数叫 `ReflectiveLoader`，否则会报「ReflectiveLoader export not found」。
- **ManualMap**：最隐蔽但也最容易出问题（不经过系统的 LoadLibrary，DLL 里的某些依赖可能无法正确加载），稳定性最低。
- 不熟悉时，**永远优先用 LoadLibraryW**。

### 小技巧
- 注入前先确认游戏已经**完全进入主界面**（窗口标题已变为游戏名），这样进程和窗口更稳定。
- 一次注入多个 DLL 时，若某个失败，其它成功的不受影响，日志里会分别列出成功与失败项。

---

## 更新日志

### v1.0.2（2026-08-24）稳定性与隐蔽性增强版
本版本聚焦注入稳定性与隐蔽性（供反作弊研究/测试参考）：

- **新增 `module_hide` 模块隐藏**：被注入的 DLL 会从目标进程 PEB 的 InLoadOrder / InMemoryOrder / InInitializationOrder 三条加载链表中摘除，并清空名字字符串，使 `GetModuleHandle`、模块枚举等无法再发现该模块——对标主流注入脚本的常见收尾手段。已接入 `core.dll` 与 `jar_loader`。
- **重写 `jar_loader` 的 JNI 代码**：按 `jni 0.21` 正确 API 重写，修复编译与链接问题（`JNI_GetCreatedJavaVMs` 改为从 `jvm.dll` 动态解析），使其可正常构建并打进发布包。
- **明确仅支持 64 位**：`check_dll_architecture` 现在对 32 位 DLL 或 32 位目标进程直接报错，消除「UI 放行、底层却报 Not 64-bit」的自相矛盾。
- **ManualMap 节区保护位映射**：按 PE `Characteristics` 正确映射为 `PAGE_EXECUTE_READ` / `PAGE_READWRITE` / `PAGE_NOACCESS` 等。
- **Reflective 重定位缺失**：注入前检测 `delta`，非零时调用重定位修复逻辑，避免基址不匹配导致崩溃。
- **32 位 PE 误解析**：检查 `OptionalHeader.Magic`，遇到 32 位 DLL 时明确报错返回，避免越界读取。
- **GetProcAddress ordinal 传参**：改用 `MAKEINTRESOURCEA` 语义，避免小序号被当作内存地址解引用。
- **窗口标题获取废代码**：重写为带状态的回调，找到目标 PID 的窗口后提取标题并立即终止枚举。
- **进程选择 PID 解析错位**：改用索引精确匹配，杜绝进程名/标题含空格时的解析歧义。
- **版本号统一**：日志版本号改用 `env!("CARGO_PKG_VERSION")`，与 `Cargo.toml` 自动同步。

---

## 参与开发 / 二次开发

- 本工具用 **Rust** 编写，改动后运行 `cargo build --release` 重新编译。
- 注入逻辑集中在 `src/injector/`，新增注入方式可在 `src/injector/mod.rs` 的 `InjectMethod` 里扩展。
- 进程筛选逻辑在 `src/process_finder.rs`。

---

## 许可

本项目使用 **Apache-2.0** 开源协议。Copyright © 2026 FastNow Studio。

> **请勿用于任何非法用途**：未经授权向他人电脑/游戏注入 DLL 属于违法行为。请遵守当地法律与游戏服务条款。
