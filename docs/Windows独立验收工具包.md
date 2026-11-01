# Windows 独立验收工具包


2026-10-20 最新工具包：`dist/windows-acceptance-folders/windows-acceptance-kit.zip`，SHA256 `d26b1e04618071c344d6d29cbc3f6a5298ee0862faf0051b8563a14b775fe3e3`，只绑定 `dist/0.1.0-local-test-20261020-folders/media-browser-portable.zip` / EXE `845fa11a…`。当前 Windows PowerShell 5.1 下 PNG/H264 MP4 两例真实画面、音频/seek自检通过；这是部分运行，未运行全部24例，也不是Win11/干净环境验收。原始验证见 `artifacts/folders-acceptance-kit-verification.json`。以下 overlay/94 工具包保留为历史，不能混用便携ZIP。


当前工具包：`dist/windows-acceptance-overlay-final/windows-acceptance-kit.zip`，SHA256 `d7aff14c173154b30be8efb72a1a27368f5d223a5719956b425d7bc50e78ea3c`，绑定当前 `dist/0.1.0-local-test-20261019-overlay-final/media-browser-portable.zip`。24个合法公开/生成病例，不含私有HEIC；本机PowerShell5.1 PNG与H.264两例自检通过（`artifacts/overlay-acceptance-kit-verification.json`），部分结果的full_suite_completed与checks_pass仍为false。以下release94-v2记录保留历史，不与当前包混用。


更新：2026-10-19。此工具为后续干净 Windows 10/11 实测准备，不代表这些环境已经验收。
用户已确认暂时没有 Win11 条件，本轮保留 Win11 未验证，不要求现在提供结果。

有效包：`dist/windows-acceptance-release94-v2/windows-acceptance-kit.zip`，SHA256：
`82a69aeb0d656237dcb6081ffbef8d08ee7502eaebbc2c23fe0ccbcf3fb96f33`。
搭配 `dist/0.1.0-local-test-20261019-cloud/media-browser-portable.zip`。不使用没有 v2 的旧工具包。

## 执行

将工具包解压到一个本地目录，与便携 ZIP 一起复制到待验收机器。使用 Windows 自带 PowerShell 5.1，普通用户即可；不要求 Python、Rust、系统 HEIC 扩展或外部播放器。测试会播放有声音频信号。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\windows-acceptance-kit\run.ps1 -PortableZip .\media-browser-portable.zip -OutputRoot .\acceptance-result
```

输出目录必须不存在，避免覆盖文件。脚本只使用自带的 24 个合法测试样本，不扫描用户媒体库，也不执行旋转或回收操作。它验证包和样本哈希，解压便携程序，再逐项运行原生封面、详情、视频音频会话与快进截图检查。若进程没有如期退出，保留 PID 和现场供检查，不因观测超时重复启动。

只跑部分样本用于排障时，可在 PowerShell 内运行：

```powershell
& .\windows-acceptance-kit\run.ps1 -PortableZip .\media-browser-portable.zip -CaseName @('png','h264-mp4') -OutputRoot .\acceptance-subset
```

部分样本通过仅会设置 `selected_cases_pass=true`；`full_suite_completed` 和整体 `checks_pass` 仍为 false，避免将两项测试当作全套通过。

## 返回证据与解释范围

结果入口为输出目录 `result.json`，其余原生报告、截图与日志保留在对应病例目录中。需要分享证据时可省略大体积 `application` 目录；保留结果、病例日志和截图。分享前检查系统信息是否含个人信息。

本机 Win10 的 PowerShell 5.1 已实际运行 PNG 与 H.264/AAC MP4 两例，均通过原生截图、源哈希和退出码检查；视频另含音频信号与 seek 后新画面。ZIP 完整性及 24 个样本哈希通过，证据为 `artifacts/release94-acceptance-kit-verification.json`。初版因脚本退出码采集空值误报，原始失败保留在 `target/acceptance-release94-smoke`，修正后证据在 `target/acceptance-release94-smoke-v2`，没有改写旧失败。

以上是工具完整性和本机两例自检。应用的 24+2 格式复验另见《交付构建格式与性能复验.md》。工具未在 Win11 或干净断网机器执行，不替代物理键鼠、安装卸载、DPI、多选回收、真实手机 HDR、专业颜色或显示器呈现验收。工具包不包含用户提供的私有 HEIC。
