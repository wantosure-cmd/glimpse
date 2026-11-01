# Sandbox 离线依赖验证

更新：2026-10-19。程序仍为 Release `66FEC895730A4B8ADA250E95F608FEC5180FE71B8574386081DC8FCFC7EB7568`；本轮只新增诊断工具、实测与报告，没有修改应用或更换渲染后端。

结论：**隔离环境中的离线安装、包内HEIC/MOV解码、快进和卸载通过；图形界面启动失败，完整离线验收未通过。** 不能把部分依赖通过记成A22整项通过。

## 环境与边界

本机已有Windows Sandbox程序；只读查询可选功能时因未提升管理员权限被拒，但随后实际成功启动来宾。这比exe存在或功能查询结果更能证明本次隔离环境确实运行。不启用系统功能，不修改LongPathsEnabled、驱动或全局设置。

按 [Microsoft官方Sandbox配置说明](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)，本次配置网络/剪贴板/麦克风/摄像头/打印机禁用、vGPU启用、6GB内存；程序和专用生成/合法上游样本只读映射，仅新建target目录用于取回结果。来宾自己将程序复制到本地C盘再运行，没有映射用户媒体库。

| 项目 | 宿主 | 本次来宾 |
|---|---|---|
| OS | Win10 Education 22H2 build19045.6456 | Win10 Enterprise，CIM版本10.0.19041/build19041 |
| 账户/机器 | 用户原工作环境 | WDAGUtilityAccount / Microsoft Virtual Machine |
| 图形能力实测 | NVIDIA OpenGL4.6 | GDI Generic / OpenGL1.1.0 |
| 网络 | 不据宿主状态推断 | 配置禁用，来宾没有IPv4/IPv6默认路由 |
| 工具/扩展 | 本机有开发环境 | PATH未发现Python/Cargo/Rust/Node/FFmpeg/FFprobe，未发现HEIF/HEVC商店扩展包 |

来宾CIM列出“NVIDIA GeForce RTX2070”虚拟设备，不代表WGL可用NVIDIA OpenGL4.6。独立隐藏WGL上下文探针返回的实际renderer/version才用于本次诊断。来宾build19041也不能冒充规格的Win10 22H2或Win11验收基线。

## 两次实际运行

第一次：`artifacts/sandbox-probe-f2db05b2aa334c0cb532a558d7b982d2.json`。408个便携载荷文件逐个核对，HEIC解码1280×854，MOV时长65秒及四次null输出seek通过；PNG原生UI退出码1，stderr为 `egui_glow requires opengl 2.0+`。保留失败，不把null输出当GPU或音频证明。

第二次：`artifacts/sandbox-probe-84d15a36b69c407a89ce3a5012649f1d.json`。先确认上次专属客户端已关闭，再创建全新的UUID配置和来宾：

| 检查 | 实际结果 |
|---|---|
| 输入/程序身份 | 配置、脚本、样本、安装器和应用哈希核对一致 |
| 便携载荷 | 408个文件逐个核对通过 |
| HEIC | 包内libheif1.23.6，解码1280×854；输出PNG可独立读取 |
| MOV | 包内libmpv，H.264/AAC，65秒；5/35/30/0秒四次null输出seek通过 |
| 独立WGL诊断 | 可建立旧式上下文，实际OpenGL1.1.0 / Microsoft / GDI Generic，低于2.0 |
| 离线安装 | 原生安装器静默安装到新的来宾目录，409安装文件逐个核对，卸载登记存在 |
| 安装后运行库 | 安装目录程序的HEIC解码、MOV null输出seek通过 |
| 卸载 | 卸载标记/卸载程序移除，当前用户卸载登记清理通过 |
| 原生PNG UI | 实际启动失败：egui_glow需要OpenGL2.0+；未产生有效渲染截图，未通过 |

总体 `checks_pass=false`、`full_offline_acceptance_pass=false`。独立证据检查器将“记录完整性通过”单列为 `evidence_integrity_checks_pass=true`，没有把已知失败包装为成功。MOV声音、视频FBO、完整媒体流程和输入均不能从本次null探针推断。

原始日志、解码PNG与来宾JSON在对应 `target/sandbox-probe-UUID/output`；配置/输入清单在同目录input和probe.wsb。第二次证据汇总：`artifacts/sandbox-offline-verification.json`。宿主对照为 `artifacts/opengl-probe-host.json`。图形探针静态CRT，仅依赖系统OPENGL32/GDI32/USER32/KERNEL32，不要求来宾安装编译工具。

## 结束与复现

Win10 Sandbox内的shutdown请求没有结束第一轮宿主客户端。核对启动器命令行确实指向本轮UUID配置、客户端父PID确实归属后，已关闭本轮专属会话；第二轮工具自动执行相同归属检查再关闭。两次均确认没有剩余WindowsSandbox客户端；没有终止其他虚拟机或用户程序。客户端命令行里的临时来宾凭据不保留在诊断档案中。

```powershell
cmake -S tools/diagnostics -B target/diagnostics
cmake --build target/diagnostics --config Release --target opengl-probe
python -X utf8 tools/run_sandbox_probe.py
```

CMake不在PATH时使用Visual Studio附带的CMake路径（本机可由target/diagnostics/CMakeCache.txt中的CMAKE_COMMAND定位）。启动工具会拒绝覆盖已有Sandbox会话，只创建新目录；来宾脚本先检查WDAGUtilityAccount与非宿主机器名，不能直接在宿主执行。它不启用可选功能，不自动重试失败，不安装驱动或系统扩展。

本次独立核对：

```powershell
python -X utf8 tools/verify_sandbox_probe.py artifacts/sandbox-probe-84d15a36b69c407a89ce3a5012649f1d.json --output artifacts/sandbox-offline-verification.json
```

若程序、目录或图形驱动变化，须检查新证据，不能把这个固定失败断言的诊断检查器用成通用“必须通过”的产品测试。当前还需要符合规格、能够提供OpenGL2.0及以上的干净Win10/Win11环境验证完整UI/音视频/输入。用户已明确暂没有Win11条件，继续保留未验证，不重复要求现在提供。真实手机10-bit/HDR与完整物理输入等原验收余项没有删除。

源码刷新后的新解压构建与包核对见 `artifacts/sandbox-source-build.json`、`sandbox-delivery-verification.json`。应用、便携ZIP和安装器保持原来的66构建及其已验证哈希；新增工具和报告进入应用源码包，并在verification下保存新证据快照。
