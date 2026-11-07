# Windows x64 构建说明

当前源码版本0.4.51，正在核验；最新已交付本地测试包0.4.50。实际版本以Cargo.toml、对应delivery.json与同构建verification为准；完整源码包的生产输入须逐项匹配production_source_sha256。现有工具链/缓存下的离线重建与干净机器离线首次构建分开记录，构建成功也不代表已完成交付。推进状态见[v0.5推进记录](v0.5公开测试版推进.md)。

本项目使用 Rust/eframe/egui/glow 与内嵌 libmpv GPU 视频。源码目录必须完整，不能只复制已跟踪的 Git 文件。以下命令生成本机程序或测试结果；最终便携包、安装包和源码包还需执行独立的版本/载荷/交付校验。

## 前置条件

- Windows x64；安装 `rust-toolchain.toml` 指定的 Rust MSVC 工具链，当前为 `1.98.0-x86_64-pc-windows-msvc`。
- Visual Studio C++ x64 构建工具、Windows SDK 与 Windows 版 PowerShell 7。本机已验证 MSVC 14.44.35207、SDK 10.0.26100.0；不把该记录当作其他机器已安装的证明。
- Cargo registry/cache 已有 `Cargo.lock` 的全部依赖；离线构建入口不会自动安装组件。首次获取依赖可在准备环境阶段显式执行 `cargo fetch --locked`，其网络下载与之后的离线构建分开记录。
- `assets`、`vendor`、锁定原生运行库及其清单完整。`test-media` 中一些原生格式/交互测试样本是单独准备的，本地私人测试源不自动公开分发；缺少样本时记录未执行，不能把跳过当作通过。

运行 `tools/handoff_preflight.ps1` 可检查迁移后的基础状态；迁移清单只用于接收基线，合法开发后的源码变更不要求回退。开源许可尚未由用户选择，构建成功不代表获得公开发布项目的授权。

## 构建入口

`tools/build_windows.ps1` 从仓库自身路径定位源码，自动使用 `vswhere` 或标准 Build Tools 位置；自定义安装路径可通过 `-VisualStudioPath` 指定。它先确认锁定工具链已安装，再用 `--locked --offline` 调用 Cargo，日志与退出码 JSON 写入项目 `artifacts`。工具只在当前进程调整环境，不更改系统设置。

在Windows仓库根目录用PowerShell 7执行：

```powershell
pwsh -NoProfile -File tools/build_windows.ps1 -CheckOnly
python -X utf8 tools/check_source.py
cargo fetch --locked
pwsh -NoProfile -File tools/build_windows.ps1 -Stage app
pwsh -NoProfile -File tools/build_windows.ps1 -Stage lib-tests
pwsh -NoProfile -File tools/build_windows.ps1 -Stage app-tests
```

`cargo fetch`首次联网准备依赖；构建入口随后使用`--locked --offline`。原生运行库仍须按[运行库恢复说明](构建说明.md)准备，源代码检查不代替原生运行验证。若源码和交付目录分开，可显式执行`pwsh -File tools/handoff_preflight.ps1 -DeliveryDirectory C:/path/to/dist/同版本目录 -CheckCargoCache`；没有迁移包时不要求伪造`PROJECT-TRANSFER.json`。

以下WSL路径仅保留为历史示例；使用WSL时改为本机实际存在的Windows PowerShell路径：

```bash
win_pwsh='/mnt/c/Users/Wantosure/.cache/codex-runtimes/codex-primary-runtime/dependencies/native/powershell/pwsh.exe'
"$win_pwsh" -NoProfile -File tools/build_windows.ps1 -CheckOnly
"$win_pwsh" -NoProfile -File tools/build_windows.ps1 -Stage app
"$win_pwsh" -NoProfile -File tools/build_windows.ps1 -Stage lib-tests
"$win_pwsh" -NoProfile -File tools/build_windows.ps1 -Stage app-tests
```

`CheckOnly` 仅检查已安装环境，不下载、安装或构建；成功不等于应用验收。`app` 的产物是 `target/release/media-browser.exe`。它不自动组成完整可分发运行时目录。

安装器基础程序使用静态 MSVC CRT 构建：

```bash
"$win_pwsh" -NoProfile -File tools/build_windows.ps1 -Stage setup
```

此阶段调用 `tools/build_setup.ps1`，不会自动生成最终携带载荷的安装包；最终组包按 `tools/package_local.py` 的实际参数及版本检查执行。新增二进制交付必须递增应用及安装器版本，并使程序、源码、delivery.json、同构建 verification 相互匹配。不能用内部构建或仅文档刷新冒充新交付，也不再制作完整迁移包。

## 当前验证证据

2026-10-24 本机已执行新入口的 `CheckOnly` 与 `app`：`artifacts/windows-build-environment-20261024T002248037.json`、`artifacts/windows-app-20261024T002259442.log.result.json`。离线 release 构建退出码 0，程序 SHA256 保持 `6b548cd36dde5cf4adabbc86c474aa4c8de1bbec15dca6a2978f3e6a43e4cc11`，生产输入清单 161 项未变。

上述初始构建阶段的库测试与主程序测试由此前同机入口执行，分别 86 通过/1 忽略、28 通过/2 忽略；没有用环境检查替代测试。该阶段尚未独立执行新包装分支，不能把它当作后续构建结果。优化前原生核心回归和 24 格式矩阵的哈希证据见 [0.4.20 本机验收记录](0.4.20本机验收记录.md)。

后续快滚方案 3 已实际通过新入口 `app-tests`（32 通过、2 忽略）与 `app`，日志为 `artifacts/windows-app-tests-20261024T094128333.log`、`artifacts/windows-app-20261024T094501982.log`。内部主程序 SHA256 为 `1130401f0c21230c44495642f5fa8f3474dd3638a15e240b9140340de24b9769`，六组受控核心回归及 24/24 格式矩阵见 `artifacts/win11-v0420-geo3-core-regression.json`。`lib-tests` 包装分支尚未独立补跑，不伪称已跑。

同版本安装器基础程序也已使用 `-Stage setup` 完成首次本机静态 CRT 离线构建，日志 `artifacts/windows-setup-20261024T101903482.log`，退出码 0。基础程序 SHA256 `519a238764cc88372a28df0d49a3f749d80c44cd96e61426325ea5d96ce474a1`。只读版本/哈希/PE 导入检查见 `artifacts/win11-v0420-setup-stub-check.json`：Cargo、主程序与安装器 FileVersion 均为 0.4.20，安装器无直接动态 MSVC CRT 导入，主程序字节未改变。这不等于带载荷安装器已形成或实际安装/卸载通过。

性能、离线安装/卸载、干净系统、实体键鼠/听感和 HDR 参考的验证范围分别记录；构建或单元测试通过不替代它们。

## 内部载荷预验证

`tools/package_local.py --payload-only` 生成专用 `staging.json`，标记 `internal-payload-test-only`，不生成正式 `delivery.json` 或最终便携/源码 ZIP，也不占用正式版本交付记录。安装器哈希、应用哈希、版本、生产输入和许可证完整性在内部清单中单独登记。安装核验须显式加 `--allow-staging`，此模式禁止 `--registry`；最终交付仍使用正式清单路径，不能把内部清单改名冒充交付。

本机内部目录 `dist/0.4.20-internal-payload-20261024-r2` 已实际通过安装/卸载：450 文件逐项哈希、安装后的 HEIC 解码/MOV 探测、非空/联接/运行中拒绝、未知与修改文件保留和设置保留。证据 `artifacts/win11-v0420-internal-installation.json` 明确标记内部测试，`final_delivery_acceptance=false`。205 项 Windows Rust 依赖许可证文本无缺口。没有登记注册表或变更 Explorer 入口，不代表干净断网系统认证。

首次暂存 `dist/0.4.20-internal-payload-20261024` 因缺少锁定许可证文本被校验拒绝，原目录保留。使用 SHA256 为 `e30b6d305099d746c95b934ccd8ea0e0e902fdc75d29b318e832855b934a8589` 的原 0.4.18 便携 ZIP，通过 `tools/restore_runtime.py` 完整核验 24 项后仅补回 6 份缺失文本：libde265/libheif/libmpv/NotoSansSC，以及 jpeg-decoder 的 Apache/MIT 文本。已有文件哈希不同会拒绝覆盖；没有修改运行库 DLL 或程序。随后在新的 r2 目录组装成功。新增内部清单门禁 3 项测试通过。
