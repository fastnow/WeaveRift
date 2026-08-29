# 构建脚本使用帮助

本文档帮助你理解并上手本项目的**两个构建脚本**：

| 脚本 | 位置 | 作用 | 谁用 |
|------|------|------|------|
| 一键打包脚本 | `package.bat` | 本机编译并生成 `release/` 文件夹 | 想在自己电脑上出包的人 |
| 自动发布工作流 | `.github/workflows/auto-release.yml` | GitHub 云端自动编译 + 打包 + 发布 Release | 想每次更新自动发版的人 |

下面分别讲解。

---

## 一、本地一键打包：`package.bat`

### 它是干什么的？
双击运行后，它会在你本机自动完成：
1. 清空旧的 `release` 文件夹
2. 编译 **Release 版主程序**（`WeaveRift.exe`）
3. 编译覆盖层 `WeaveRift/core.dll`
4. 把 `exe`、`icon.ico`、`core.dll` 按正确目录结构放到 **`release/`** 文件夹里

> `release/` 就是可以直接分发 / 压缩给别人使用的成品目录。

### 使用前提（必须先装好）
- **Rust**（必须）：去 <https://www.rust-lang.org/tools/install> 安装，装完会有 `cargo` 命令。
- **Windows 系统**（本工具只支持 Windows）。
- 在 **cmd 或 PowerShell** 中确认能运行 `cargo --version`，能输出版本号就说明装好了。

### 怎么用
最简单的方式：**双击 `package.bat`**。
也可以打开终端，进入项目目录后运行：

```bat
package.bat
```

运行时会打印步骤进度，最后输出成品所在目录，并列出内容：

```
================================================
   Build Complete!
================================================
Output folder: ...\WeaveRift\release
```

### 运行过程失败怎么办？
`package.bat` 里每一步都会检查 `errorlevel`，出错会暂停并提示。常见原因：

| 现象 | 原因 | 解决 |
|------|------|------|
| `[ERROR] cargo not found` | 没装 Rust 或没加入 PATH | 安装 Rust 并重开终端 |
| `[ERROR] Build failed` | 代码编译报错 | 看终端里的编译错误信息，或 `cargo build --release` 单独排查 |
| `[ERROR] Core build failed` | `WeaveRift` 覆盖层编译失败 | 进入 `WeaveRift` 目录单独 `cargo build --release` 排查 |

> 你也可以手动编译主程序（等价于 `package.bat` 第 2 步）：
> ```
> cargo build --release
> ```
> 生成文件在 `target\release\WeaveRift.exe`。

---

## 二、自动发布：GitHub Actions 工作流

文件名：`.github/workflows/auto-release.yml`

### 它是干什么的？
当你把代码推送到 GitHub，且**最新一条提交信息包含 `[RELEASE]`** 时，云端会自动：
1. 读取根目录 **`version.json`** 里的发布信息（`title` / `version` / `description` / `release`）
2. 在 Windows 虚拟机上编译 `WeaveRift.exe` 和 `WeaveRift/core.dll`
3. 打包成 **`release.zip`**
4. 用 `version.json` 的信息创建 **GitHub Release**（附件是 `release.zip`）

**你完全不用在自己电脑装 Rust**，只要改好 `version.json` 并 push 就行。

### 📄 先认识 `version.json`（发布信息的唯一来源）

发布时的**版本号、标题、说明、是否发布**全部由根目录的 `version.json` 控制。结构如下：

```json
{
  "title": "WeaveRift v1.0.3",
  "version": "1.0.3",
  "description": "本版本主要更新：\n- 修复进程筛选问题\n- 新增功能",
  "release": true
}
```

字段说明：

| 字段 | 是否必填 | 作用 |
|------|:---:|------|
| `title` | 可选 | Release 标题；缺省时自动用 `v<version>` |
| `version` | **必填** | Release 的 tag / 版本号（会生成 tag `v2.3.0`）；缺省会直接报错 |
| `description` | 可选 | Release 更新说明正文（支持 `\n` 换行） |
| `release` | 可选 | **布尔开关**：`true` 才真正创建 Release，`false`/缺省则只编译打包、不发布 |

### 触发与使用

**步骤 1：更新 `version.json`**
把 `version`、`title`、`description` 改成新版本的内容，并确认 `release` 为 `true`。

**步骤 2：提交信息带 `[RELEASE]` 并推送**
```bash
git add version.json
git commit -m "[RELEASE]"
git push
```

**步骤 3（可选）：手动触发测试**
- 打开 GitHub 仓库 → `Actions` → `Auto Release` → 右侧 **Run workflow** → 运行。

### 产物结构（`release.zip` 内）
```
release.zip
└── WeaveRift.exe   # 主程序
├── icon.ico               # 图标（存在才打包）
└── WeaveRift/
    └── core.dll           # 开场动画覆盖层
```

### 工作流里发生了什么（逐步对照）
1. `Checkout` — 拉取代码
2. `Set up Rust` — 装 Rust 稳定版工具链
3. `Rust cache` — 缓存编译产物，加速后续构建
4. `Read version.json metadata` — 读取 `title`/`version`/`description`/`release`，把说明写入 `RELEASE_BODY.md`
5. `Build WeaveRift.exe` — 编译主程序
6. `Build WeaveRift/core.dll` — 编译覆盖层
7. `Assemble release folder` — 组装 `release/` 目录
8. `Compress to release.zip` — 用 `Compress-Archive` 压缩
9. `Create GitHub Release` — **仅当 `release=true`** 时创建 Release 并上传 `release.zip`（标题、tag、说明都来自 `version.json`）

### 首次使用前要做的配置
1. **把代码推到 GitHub**：本项目还没配置远程仓库，先关联：
   ```bash
   git remote add origin https://github.com/<你的用户名>/<仓库名>.git
   git push -u origin main
   ```
2. **确认 Actions 权限**：仓库 `Settings → Actions → General → Workflow permissions`，勾选 **Read and write permissions**（工作流需要 `contents: write` 才能创建 Release，文件里已声明该权限）。
3. **确认默认分支名**：工作流监听 `main` 和 `master`，如果你的分支不叫这两个，需要改文件第 12 行的 `branches`。

### 常见问题

| 现象 | 原因 | 解决 |
|------|------|------|
| 提交了 `[RELEASE]` 但没触发 | 不是最新一条提交，或分支不在 `main`/`master` | 让含 `[RELEASE]` 的提交成为本次 push 的最新一条；确认分支名 |
| 工作流报 `version.json is missing required field: version` | `version.json` 里没有 `version` 字段 | 补上 `"version": "x.y.z"` |
| 编译打包了但没发布 Release | `version.json` 的 `release` 不是 `true` | 把 `release` 改成 `true` 再 push |
| 创建 Release 失败 / tag 重复 | 版本号已被用过 | 换一个新版本号 |
| Release 标题/说明不对 | `version.json` 没改 | 先更新 `title` / `description` 再触发 |
| 构建失败 | 代码编译报错 | 查看工作流日志里的 `Build` 步骤输出 |

---

## 🔧 三、两者如何选

| 你的需求 | 用哪个 |
|----------|--------|
| 本机快速出一份 `release/` 给人用 | `package.bat` |
| 每次更新自动出正式 Release 发布到 GitHub | GitHub Actions 工作流 |
| 想在自己电脑上调试编译报错 | 手动 `cargo build --release` |

> 提示：本地调试编译用 `cargo build`（debug 快）；出正式包用 `cargo build --release` 或 `package.bat`（更小更快，已配置 `lto`、`strip`、`opt-level=z`）。
