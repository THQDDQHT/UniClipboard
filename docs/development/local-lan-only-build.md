# v0.19.3 局域网修复版

本分支 `feat/v0.19.3-lan-fix` 基于正式标签 `v0.19.3`，版本号为
`0.19.3-lan.2`（版本号以 `package.json` 为准）。保留该版本的界面和协议，不合入后续主界面改版。
原分支 `feat/lan-only-local-fix` 保留不动。

## 迁移范围

- 将个人 Engine 修复提交 `97a27537`、`e9632b64`、`b799c0ec` 中的地址过滤、
  配对组播、已配对设备发现及网络切换恢复逻辑，适配到旧版现有基础设施层。
  旧版尚未拆分 Engine，因此保持原有工作区结构，不引入新版 Engine API。
- 地址过滤与两种 mDNS 发现共用当前网卡规则，排除已知容器与宿主私有虚拟网卡，
  保留物理网桥；网络变化后重新读取网卡。
- `swarm-discovery` 与 `iroh-mdns-address-lookup` 固定到个人 Engine 的
  `b799c0ecac4101af72ca7b9aeced58b4d34a34f8`，不复制第三方补丁。
- `iroh-blobs` 固定到原有子模块的同一个提交，方便 GitHub 干净检出直接构建。
- 新版切换空间错误提示依赖后续准入流程，旧版没有该界面，因此不迁入。
- 不改变旧版加密、身份验证与中继开关的语义，但新安装默认开启「仅局域网模式」
  （`network.allow_relay_fallback` 默认值为 `false`）。已有的设置文件显式保存了该字段，
  不受影响。需要中继回退时，在网络设置中关闭「仅局域网模式」并按提示重启后台服务；
  仅在关闭该模式后才会显示自定义中继节点设置。

这是对既有结构性网络缺陷的回移：用共享网卡分类替换重复枚举和启动快照，
没有增加另一套并行发现流程。第三方补丁依赖固定提交，便于审计与重现。

## GitHub 构建与发布

发布版由推送标签触发。在 fork 中更新版本号（`Cargo.toml`、`src-tauri/Cargo.toml`、
`package.json`、`src-tauri/tauri.conf.json` 与 `Cargo.lock`）后，推送形如
`v<版本号>` 的标签，例如 `v0.19.3-lan.2`：

```bash
git tag v0.19.3-lan.2
git push origin v0.19.3-lan.2
```

标签推送会构建 Apple Silicon macOS `.dmg` 和 Windows x64 便携版 ZIP，
全部成功后自动创建同名 GitHub Release，仅附带 `.dmg`、`*-portable.zip` 和覆盖
这两个文件的 `SHA256SUMS.txt`（不上传 NSIS 安装程序）。缺少 `.dmg` 或便携版 ZIP
时发布步骤会失败。

也可打开 fork 的 Actions，选择 `Build personal installers`
（工作流文件为 `build.yml`）手动运行（`workflow_dispatch`）。手动运行只上传
Artifacts，不创建 Release：

- `all`：Apple Silicon macOS `.dmg` 和 Windows x64 安装程序及便携版 ZIP。
- `macos-aarch64`、`macos-x86_64`、`windows-x86_64`：分别构建单个平台。

工作流使用锁定依赖，先运行网络回归和更新检查测试，再构建相同目标架构的
`uniclipd` 与图形应用。完成后从本次运行的 Artifacts 下载对应压缩包，内含
安装程序、Windows 便携版 ZIP、`SHA256SUMS.txt` 与源码提交记录 `SOURCE.txt`。保留期为 30 天；
未配置发布到官方渠道，也不会修改上游仓库。

本构建不需要上游签名私钥或 Apple 开发者证书。macOS 使用 ad-hoc 签名，
没有 Apple 公证；Windows 安装程序没有代码签名。

## 本地打包

需要 Node.js、Bun、Rust 1.95.0，以及对应平台的原生构建工具。

```bash
bun install --frozen-lockfile
bun run tauri:build:local --bundles app,dmg
```

Windows 使用：

```powershell
bun run tauri:build:local --bundles nsis
```

可用 `--target <triple>` 显式指定架构；脚本会将同一目标传给后台与图形应用。

## 更新与数据降级

本分支默认启用 `manual-updates`：后台、托盘与设置页共用的更新检查入口
直接返回无可用更新，不访问官方更新源，避免个人修复版被官方包替换。
安装包不生成自动更新签名文件，后续通过手动下载个人安装包升级。

构建不会操作本机正在运行的应用及数据。应用标识沿用旧版
`app.uniclipboard.desktop`，所以 **不要把旧版直接覆盖到已被 1.x 升级的数据上**。
降级前应完整退出图形应用与后台，备份整个数据目录；先用独立 `UC_PROFILE`
验证，再使用与 v0.19.3 兼容的旧备份或建立新的空间并重新配对。
新旧版本的数据格式与配对协议不能视为可逆；两端应使用相同个人构建。

## Windows 便携版

Windows 构建同时生成 `UniClipboard_0.19.3-lan.2_x64-portable.zip`。
完整解压到可写目录，双击 `UniClipboard.exe` 运行；`uniclipd.exe` 与
`portable.dat` 必须保留在同一目录。应用数据、密钥、搜索索引及日志位于
程序旁的 `data` 目录，沿用既有加密方式。首次使用需要重新配置和配对，
不会自动迁入安装版的数据。

需要 Microsoft Edge WebView2 Runtime。WebView2 浏览器缓存仍可能写入
系统用户目录，因此便携版不等同于完全无系统痕迹。官方更新检查同样停用。

在 Windows 本地完成 `bun run tauri:build:local --target x86_64-pc-windows-msvc --bundles nsis`
后，可以单独生成便携包：

```powershell
python scripts/build-windows-portable.py --target x86_64-pc-windows-msvc
```
