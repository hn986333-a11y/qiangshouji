# 果到雷达 SOCKS5 定制源码

本包是可审阅、可继续构建的源码修改，**不是已验收的 Windows EXE**。

## 核验来源

- 公开仓库：https://github.com/suversal/apple-store-inventory-monitor
- 本次基线提交：`363601e33e0354907242ac8677680ee73b971e37`，源码版本 1.0.22。
- 项目名称为“果到雷达”，Rust / Tauri 2 / React，GPL-3.0-or-later；不是 Apple 官方软件。
- 源码通过 GitHub API 下载，提交号经 API 核对。保留 LICENSE、NOTICE 和来源信息。

## 改动

1. 查询线路界面改为“跟随系统代理 / 内置 SOCKS5”。原有 Clash 配置保留读取兼容，新的 SOCKS5 模式不需要安装或配置 Clash。
2. 每行导入 `IP:port:user:password`，最多 100 条，重复项去重；整批校验失败不覆盖原列表。支持域名、`[IPv6]`、空认证和密码中的冒号。
3. 检测完成 SOCKS5 协议协商、认证和到 `www.apple.com:443` 的 TCP 隧道。显示可用/失败/超时及耗时。它不请求库存，也不验证 Apple 是否允许库存查询。
4. 手动选择并保存首选节点，关闭原查询浏览器连接，后续查询使用新节点。界面显示当前运行节点。
5. 内置仅监听 127.0.0.1 随机端口的 HTTP CONNECT → SOCKS5 转接层，解决 Chromium 不直接支持 SOCKS5 用户密码的问题。凭据不进入浏览器启动参数。仅允许 Apple 域名的 443 端口，限制并发连接和单隧道存活时间。
6. 只有代理 TCP 连接失败、连接超时或 SOCKS 握手读写失败时尝试备用节点。认证失败、目标拒绝和已经建立隧道后的错误不会换节点重放。无可用节点时查询失败，不自动直连。
7. 代理列表、认证信息、首选节点、备用开关通过原配置存储机制持久化。故障时选出的运行节点只保存在本次进程，重启恢复用户保存的首选节点。
8. 库存接口 403 / 429 / 541、被识别的拒绝响应及送货接口拒绝，会暂停后续库存查询，按 60、120、240、480、960、1920、3600 秒递增退避，之后上限 3600 秒。本次进程内跨地区、跨轮次保留；修改线路和“立即重试”不会解除。退避到期仅允许恢复探测，不保证 Apple 已解除限制。退避没有解析服务端 Retry-After 头。
9. 目录刷新不再在代理失败后改用系统网络。目录发现/取页遇到拒绝或限流会停止后续目录取页并触发同样的退避；核心 HTTP 重试不再快速重试限流错误。
10. 库存、提醒、购物袋和结账源码保留；未修改 checkout 模块，也未开启自动提交订单。代理改动针对库存查询和目录刷新，结账浏览器仍沿用上游网络行为。
11. 定制版本关闭上游更新检查和签名更新产物，避免官方更新覆盖修改。安装包未签名。

凭据随原有配置以明文保存。Windows 使用 `%APPDATA%\apple-store-inventory-monitor` 下的原配置目录；安装前先备份现有配置，不要分享含密码的配置文件。调试格式化输出隐藏 SOCKS 凭据，前端列表不显示密码。

## 已完成的验证与限制

- TypeScript 类型检查：通过。
- 前端生产构建 `pnpm build`：通过。Vite 配置改用标准 ESM URL 路径，使用 runner 配置加载器。
- 前端测试：55 项通过，其中新增 3 项导入测试。
- `cargo fmt --all --check`：通过；只能证明 Rust 源码可格式化，不能替代编译。
- 已增加 SOCKS 认证、远程 DNS、故障备用、凭据落盘和退避测试；更新了原来“拒绝后换 IP”的测试预期。**Rust 测试尚未运行通过**。
- `cargo check --workspace`：在编译依赖构建脚本时失败。当前 Mac 缺少 Xcode Command Line Tools，`cc` 链接器提示 `No developer tools were found`；尚未完成应用 Rust 代码的类型检查。
- 未提供真实代理凭据，未做真实代理/Apple 库存联网验收，未运行 Windows GUI，未生成或验收 EXE。
- Rust 编译和 Windows 验收仍是交付给最终使用者前的必做步骤；本包不应当被当成已经验证可运行的安装包。

## 在 Windows x64 构建

准备 Visual Studio Build Tools 的“使用 C++ 的桌面开发”工作负载和 Windows SDK、WebView2、Rust stable MSVC（项目最低 1.90）、Node.js 24、pnpm 9。Tauri 系统依赖参考：[官方前置要求](https://v2.tauri.app/start/prerequisites/#windows)。

在解压后的项目目录打开 PowerShell，逐条执行；任一步失败就停止并处理错误：

```powershell
pnpm install --frozen-lockfile
pnpm test
cargo fmt --all --check
cargo test --workspace --locked
pnpm tauri build --bundles nsis
```

也提供 `build-windows.ps1`，它按相同步骤执行，并在命令返回非零时停止。

成功后的产物：

- 安装程序：`target\release\bundle\nsis\*-setup.exe`
- 应用程序：`target\release\apple-store-inventory-monitor.exe`

运行仍需 Chrome 或 Edge。不要用上游发布页下载的 EXE 代替本次定制构建。

## 使用 GitHub Actions 构建

把源码放入你自己的 GitHub 仓库并提交全部修改后，在 Actions 中手动运行 **Build private SOCKS5 Windows EXE**。流程位于 `.github/workflows/windows-socks5.yml`，先测试再构建，通过后下载名为 `socks5-windows-x64` 的 artifact。流程不会发布 Release。本次没有上传到任何仓库，也没有触发云端构建。

## Windows 验收

- 先通过全部 Rust 测试；安装并启动，确认原监控项、提示音和提醒测试正常。
- 选择内置 SOCKS5，导入你自己的代理列表，检测节点，然后手动选择节点开始监控。
- 关闭首选代理并保留备用代理，确认只在连接失败时使用备用；重新选择原首选时应关闭现有查询隧道。
- 重启应用，确认列表、首选节点和开关恢复。
- 使用本地模拟服务验证 403/429/541 后不切换出口、后续地区不请求、立即重试不解除退避。不要为了测试而高频请求 Apple。
- 原来的结账/手动确认功能需要单独验收；不要在验收中启用自动提交订单。
