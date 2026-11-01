# XTWS-TUI

[English](README.md) | [中文](README.zh.md)

XTWS-TUI 是一个基于 Rust 的 FT8 / FT4 弱信号数字模式终端监控工具，运行于 TUI 界面中。它可以监听音频输入、解码数字模式信号、跟踪已联系呼号与网格，并维护运行日志与 QSO 日志。

## 功能概览

- FT8 / FT4 音频采集与解码
- 终端界面显示 RX / TX / 事件信息
- 自动跟踪已工作呼号、网格和 DXCC 前缀
- QSO 状态机，支持自动 CQ 与手动回复
- 运行日志与 ADIF QSO 日志输出
- 可配置的 UI 颜色、采样率、日志目录和频率参数

## 运行环境

在 Linux 环境中，推荐满足以下条件：

- Rust 2024 edition
- ALSA 或 PipeWire 相关音频库
- 可用的音频输入输出设备，例如声卡或 USB 音频接口
- 具备终端支持的可交互 TUI 环境

如果你在 Debian / Ubuntu 系统上构建，通常需要先安装：

```bash
sudo apt install build-essential pkg-config libasound2-dev
```

## 快速开始

### 1. 安装 Rust

```bash
rustup default stable
```

### 2. 进入项目目录

```bash
cd xtws-tui
```

### 3. 构建项目

```bash
cargo build --release
```

### 4. 运行程序

```bash
cargo run --release
```

如果你已经有构建产物，也可以直接运行：

```bash
./target/release/xtws-tui
```

> 如果你使用的是发布版本二进制，通常也可以直接执行编译产物并无需额外命令。

## 配置说明

主配置文件位于 [config.toml](config.toml)。

### 最小配置

最小可用配置通常包括：

```toml
[user]
call = "JA1ABC"
grid = "PM95"
freq_mhz = 21.074

[device.radio]
protocol = "ft8"

[log.run]
dir = "logs/run"
keep_days = 30
mem_lines = 1000

[log.qso]
dir = "logs/qso"
keep_days = 3650

[log.worked]
calls_path = "logs/worked_calls.txt"
grids_path = "logs/worked_grids.txt"
```

这部分覆盖了最关键的站台身份、协议选择和日志位置。若未显式覆盖，程序会为音频、UI 和 FT8/FT4 参数提供默认值。

常见配置段说明：

- `[user]`：呼号、网格和操作频率
- `[device.radio]`：协议类型和收发模式
- `[ui]`：界面布局、刷新间隔、颜色和筛选逻辑
- `[qso]`：自动回复、重试次数和 CQ 参数
- `[log.run]` / `[log.qso]` / `[log.worked]`：日志目录与保留策略
- `[build.ft8]` / `[build.ft4]`：发射参数和采样率
- `[decode.ft8]` / `[decode.ft4]`：解码范围

示例：

```toml
[user]
call = "JA1ABC"
grid = "PM95"
freq_mhz = 21.074

[device.radio]
protocol = "ft8"
```

## 项目结构

```text
.
├── Cargo.toml
├── README.md
├── README.zh.md
├── LICENSE
├── THIRD_PARTY_NOTICES.md
├── config.toml
├── shell.nix
├── logs/
├── src/
│   ├── main.rs
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── config.rs
│   │   ├── qso.rs
│   │   └── worked.rs
│   ├── infrastructure/
│   │   ├── mod.rs
│   │   ├── audio.rs
│   │   ├── decode.rs
│   │   ├── qso_log.rs
│   │   ├── run_log.rs
│   │   ├── signal.rs
│   │   └── slot.rs
│   └── presentation/
│       ├── mod.rs
│       └── ui/
│           ├── mod.rs
│           ├── feed.rs
│           └── tui.rs
├── target/
└── logs/
```

## 核心模块职责

- [src/main.rs](src/main.rs)：应用入口，协调配置加载、音频、日志、解码与 TUI
- [src/domain/config.rs](src/domain/config.rs)：配置结构体、默认值与校验逻辑
- [src/domain/qso.rs](src/domain/qso.rs)：QSO 状态机与消息处理
- [src/domain/worked.rs](src/domain/worked.rs)：已联系呼号与网格缓存
- [src/infrastructure/audio.rs](src/infrastructure/audio.rs)：音频设备管理、采集与播放
- [src/infrastructure/decode.rs](src/infrastructure/decode.rs)：FT8 / FT4 解码封装
- [src/infrastructure/run_log.rs](src/infrastructure/run_log.rs)：运行日志记录
- [src/infrastructure/qso_log.rs](src/infrastructure/qso_log.rs)：ADIF QSO 日志导出
- [src/infrastructure/signal.rs](src/infrastructure/signal.rs)：FT8 波形生成与调制信号
- [src/infrastructure/slot.rs](src/infrastructure/slot.rs)：时隙同步与奇偶校验
- [src/presentation/ui/feed.rs](src/presentation/ui/feed.rs)：UI 文本格式化与状态标签
- [src/presentation/ui/tui.rs](src/presentation/ui/tui.rs)：终端交互界面与键盘控制

## 日志输出

程序运行时会写入以下目录与文件：

- `logs/run/`：运行日志，按日期存储
- `logs/qso/`：ADIF QSO 日志目录
- `logs/worked_calls.txt`：已联系呼号列表
- `logs/worked_grids.txt`：已联系网格列表

## 交互说明

TUI 界面中的常用快捷键：

- `q`：退出程序
- `↑ / ↓`：在面板中上下移动
- `← / →`：切换左右面板焦点
- `Enter`：在选中项上申请或发起回复

## 常见问题

### 1. 没有音频输入或输出设备

确认系统中 ALSA / PipeWire 正常工作，并检查配置中的输入输出设备名是否正确。

### 2. 编译失败

如果出现依赖或系统库错误，先确认已安装：

```bash
sudo apt install build-essential pkg-config libasound2-dev
```

### 3. 无法正常解码

请重点检查：

- 配置的频率与协议是否匹配
- 采样率是否正确
- 输入音频是否来自合法的 FT8 / FT4 信号
- 设备是否正确连接并能够采集到声音

## 许可证

本项目采用 GPL-3.0-only 许可。使用、修改和分发时，请遵守 GNU General Public License v3.0 的条款。

更多信息可参考：

- https://www.gnu.org/licenses/gpl-3.0.html
- https://spdx.org/licenses/GPL-3.0-only.html
