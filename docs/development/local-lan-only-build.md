# 局域网修复版的本地构建

本分支基于桌面 `v1.0.0-alpha.11`。网络修复位于 `THQDDQHT/Engine`，桌面根
`Cargo.toml` 和 `Cargo.lock` 固定同一个 Engine 提交。Cargo 会自动下载它，无需把
Engine 源码复制进桌面仓库；两台电脑拉取同一个桌面提交即可构建同一份修复。

## 构建准备

两端都需要 Git、Node.js LTS、Bun 和通过 rustup 安装的 Rust；仓库的 `rust-toolchain.toml`
会选择 Rust 1.95.0。首次构建需要联网下载源码、工具链和依赖。

- macOS：安装 Xcode Command Line Tools（`xcode-select --install`）。
- Windows：安装 Visual Studio 2022 Build Tools，勾选“使用 C++ 的桌面开发”及
  Windows SDK；安装 WebView2 Runtime。使用 PowerShell 执行命令。

详细安装说明见 [Tauri 构建前置条件](https://v2.tauri.app/start/prerequisites/)。

首次获取源码：

```bash
git clone --branch feat/lan-only-local-fix https://github.com/THQDDQHT/UniClipboard.git
cd UniClipboard
bun install --frozen-lockfile
```

已有仓库则执行：

```bash
git fetch origin
git switch feat/lan-only-local-fix
git pull --ff-only
bun install --frozen-lockfile
```

## 本地打包

macOS 在本机架构上构建 `.app` 和 `.dmg`：

```bash
bun run tauri:build:local
```

Windows 在本机架构上构建 NSIS 安装程序：

```powershell
bun run tauri:build:local --bundles nsis
```

脚本先构建并暂存必需的 `uniclipd` 后台程序，再打包图形应用。产物位于
`target/release/bundle/`，macOS 安装包通常在 `dmg/`，Windows 安装包在 `nsis/`。
不要只复制图形界面的可执行文件，缺少后台程序无法正常运行。

本地配置关闭自动更新签名产物生成，macOS 使用 ad-hoc 签名，不需要上游的
更新私钥或 Apple 开发者证书。它仍是 release 构建，不启用调试接口；不会更改
应用身份、数据目录或加密方式。macOS 包没有 Apple 公证，只供自己的设备使用。
签名差异见 [Tauri macOS 签名说明](https://v2.tauri.app/distribute/sign/macos/)。

## 开启仅局域网并验收

1. 两端安装修复版，在网络设置中开启“LAN-only 模式”，保持中继回退关闭。按应用
   提示重启；也可以从托盘完全退出再启动，使后台服务重新读取连接策略。仅关闭
   窗口不会退出后台服务。
2. 将两台设备接入同一个允许设备互访的局域网，允许应用和 `uniclipd` 通过系统
   防火墙。访客 Wi-Fi 的客户端隔离会阻止设备互相发现。
3. 确认设备显示“直连”，分别复制一段无敏感信息的测试文本，检查双向同步。
4. 在运行 Docker、WSL 或虚拟机时重复验证；切换 Wi-Fi 或插拔 USB 网卡后，
   检查设备是否重新连接。保留正常连接物理网络的系统桥接接口，不按整个私有
   地址段一概排除。
5. 保持局域网路由器工作，断开其互联网出口后再次测试双向同步。两端都需提前
   完成安装。此项用于验证连接不依赖互联网；真实跨设备验收需在你的 Mac 和
   Windows 上执行。

LAN-only 关闭的是 P2P 中继与公网发现，不等于关闭应用所有联网功能。建议在
设置中关闭自动检查更新、自动下载更新及不需要的遥测，避免自行构建的修复版
被官方更新替换。局域网模式继续使用现有认证加密连接与本地加密存储；本次
修复不改变文件内容本体允许按原字节落盘的既有规则。

## 修复范围

这是地址发布规则重复、网卡状态过期造成的结构性缺陷：启动时的子网快照会把
已知容器虚拟网卡算作 LAN，网络切换后仍继续使用旧数据。修复将网卡分类收敛到
Engine 的地址过滤模块，删除配对 mDNS 的重复枚举，替换启动快照；桌面仅引用
固定的 Engine 提交，不维护另一套网络规则。

- endpoint 地址发布、配对 mDNS 和邀请 ticket 共用已知虚拟网卡分类。
- 每次 Iroh 发布地址时读取当前网卡，复用其原生网络变化检测，不再固定使用
  启动时的子网快照。
- 网卡读取失败时不发布本次候选；后续发布重新读取，不自动启用中继。
- 配对 mDNS 发布和解析显式加入各个可用的 RFC1918 局域网 IPv4 接口，避免系统
  默认组播路由选中代理；解析端在五秒窗口内主动查询，IPv6 仍保留自动绑定。
- 桌面根工作区同时固定 Engine 内置的发现库补丁，避免 Cargo 忽略依赖仓库的
  `[patch.crates-io]`，导致本地测试和安装包使用不同实现。
- 保留现有身份验证、加密、mDNS 邀请分段和连接重试机制。

这不是对任意 VPN、TUN 路由或防火墙配置的自动修复。系统代理若接管局域网
路由，仍需允许本地网段直连。

## 验证记录

本机已通过 Engine 网络、配对、邀请及节点生命周期测试，共 270 项通过、2 项
忽略；Engine 全工作区编译检查和两仓库架构检查也已通过。桌面前端构建及
`uc-daemon`、`uc-webserver`、`uc-tauri` 的锁定依赖编译检查通过。

首次网络测试中有一项实时连接测试返回离线；在原始 Engine 提交单独运行该项、
在修复提交重跑网络测试集和单独重跑该项均通过，未为此修改连接逻辑。

2026-09-14 已在 Apple Silicon 上基于桌面提交 `fd8832c3` 执行
`bun run tauri:build:local`，成功生成 `.app` 和 `.dmg`。主程序与 `uniclipd`
均为 arm64，应用的 ad-hoc 签名验证和 DMG 完整性检查通过；没有 Apple 公证。
该轮未执行 Windows 原生打包或真实 Mac/Windows 双设备验收。

### 第二轮：邀请发现失败

真实 Windows/Mac 对照中，默认组播发送到 Mac 的接收数为 0，指定 Wi-Fi 后为 18；
两组 Windows 发送均成功 20 次。两端后台均报告 `invitation_unavailable`。第二轮
修复配对组播的网卡收发和查询间隔，并让切换空间弹窗显示实际准入失败原因。

Engine 配对、地址过滤、邀请适配器及 LAN-only 策略测试通过，真实 mDNS 邀请
分段收发测试通过。工作区编译检查通过，HarmonyOS 合同测试有既有未使用导入警告。
新版测试程序的广泛网络测试受到 macOS 应用防火墙授权阻塞；旧测试程序的对照
用例通过，此项待放行新测试程序后补验。前端相关 9 项测试通过，构建通过，
React Doctor 为 100/100。

第二轮桌面提交为 `d74d98e2`，Engine 为 `e9632b64`。macOS `.app` 与 `.dmg`
构建成功，arm64 架构、ad-hoc 签名验证和 DMG 完整性检查通过。DMG SHA-256：

```text
4ce64f0f339763a530760ef14ca4721ee9fce60e16eccad300fa6f9a36f5f835
```

Windows 在同一桌面提交上完成 release 后台和 NSIS 安装包构建，主程序 PE
架构为 x64。安装包为 `target/release/bundle/nsis/UniClipboard_1.0.0-alpha.11_x64-setup.exe`，
大小 20,420,155 字节，SHA-256：

```text
3387a9621a489109be9e05cc5de86d188768247f7864b74d85b012fdfe0450c7
```

两台真实设备使用独立临时配置，从启动起关闭中继和 overlay 地址，禁用系统
剪贴板，仅发送随机测试文本。Windows 发出邀请、Mac 加入的状态达到 `active`；
Mac → Windows、Windows → Mac 各有一个目标接受，接收端查询到的完整文本
与发送内容一致。测试前后均核对仅局域网设置保持关闭中继。Windows 测试程序
使用仅允许 Mac 局域网地址入站 UDP 的临时防火墙规则；测试配置、进程和规则
均已清理，未修改已有空间或剪贴板内容。

反向邀请（Mac 发出、Windows 加入）在 60 秒观察窗口内保持 `pending`。
macOS 系统日志明确显示新 `uniclipd` 的入站连接被应用防火墙挂起，等待用户
响应授权弹窗；当时 Mac 处于锁屏状态。这项需解锁并允许新程序后补验，不能
将前一轮的双向文本通过扩大为两种邀请方向都通过。尚未执行断开互联网出口、
切换 Wi-Fi、生产配置覆盖安装后的验收。
