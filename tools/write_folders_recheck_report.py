"""Generate the build-specific report from completed raw results."""
from pathlib import Path
import json
ROOT=Path(__file__).resolve().parent.parent
read=lambda n:json.loads((ROOT/'artifacts'/n).read_text(encoding='utf-8-sig'))
r=read('folders-release-recheck.json');assert r['evidence_valid']
fmt=read('folders-format-matrix.json');heic=read('folders-user-heic-verification.json');src=read('folders-source-release.json')
lines=['# 文件夹构建格式、性能与源码复验（2026-10-20）','',f'当前交付 EXE SHA256：`{r["exe_sha256"]}`，目录 `dist/0.1.0-local-test-20261020-folders`。','',
'本轮只补验同一构建，不修改交付二进制。24 个基础格式、2 张用户 HEIC、详情/字幕/预加载、独立源码 Release 及其中 2 个原生 GUI 病例有新证据。完整 A01–A24 尚未完成，不能将局部结果扩大为首版完整通过。','',
'## 当前机器与测量范围','',
'注册表和环境查询记录 AMD Ryzen 7 2700X、16 逻辑处理器、Windows 10 Education 22H2 build 19045.6456。GetPhysicallyInstalledSystemMemory 实测安装内存 16GiB。独立隐藏 WGL 探针实际创建 NVIDIA RTX 2070/OpenGL 4.6.0 NVIDIA 560.94 上下文；这是能力探针，不是应用自身 GPU 适配器归属或显示呈现证明。','',
'当前 WMI 查询被拒绝；D: 卷实测为 NTFS，容量 536873005056 字节，物理 SSD 型号和应用实际使用的 GPU 归属未独立确认。注册表另列远程显示适配器，未将所有注册项当作活动 GPU。旧机器报告仅作历史参考，不冒充当前查询。','',
'## 格式与原生画面','',
'以下均重新运行交付 Release；实际缩略图/详情 FBO、视频进程音频会话信号及 seek 后新画面通过。进程音频电平不是扬声器物理听音。','',
'| 病例 | 结果 |','|---|---|']
lines += [f'| {row["name"]} | 通过 |' for row in fmt['rows']]
lines += ['', '两张用户 HEIC 的主图均为 8-bit；不能充当真实 iPhone 10-bit/HDR 样本。原件哈希保持一致。方向与镜像按原始未变换解码及容器坐标映射复核，原有 RGB 平均误差 <12、错误变换比值 <0.4 的阈值未改。', '', '| HEIC | 清晰列表平均 RGB 误差 | 清晰详情平均 RGB 误差 |','|---|---:|---:|']
for row in heic['rows']:
 lines.append(f'| {row["file"]} | {row["mean_rgb_errors"]["list"]["container_order"]:.3f} | {row["mean_rgb_errors"]["detail"]["container_order"]:.3f} |')
lines += ['',
'首次方向比较失败已保留：原诊断抓取首个可用画面时仍是 48×64 柔化预览，却用清晰原图阈值比较。冷加载原始记录 `folders-user-heic-cold-progressive-ui.json` 保留这些纹理、时刻及像素；`folders-media-recheck.log` 保留失败断言。后续只复制应用已生成的指纹缓存，在全新数据目录重新实际运行同一 EXE 抓清晰图；缓存来源记录于 `folders-heic-clear-cache-provenance.json`，比较阈值不变。此清晰度复验明确属于暖缓存，不能冒充冷缓存首次清晰画面延迟。', '',
'详情与字幕复验包括浮层信息、左右时间/单行进度、缩略图首尾居中、EOF 三策略、SRT 自动/手动加载与样式、下一图片缓存及下一视频头尾 4718592 字节预读；属于受控 egui 输入和原生 FBO，不是物理输入。', '',
'## Release 性能原始结果','',
'固定 5000 份合法生成副本（4990 图片/10 视频，约 100 个分组），不代表 5000 张独立摄影原片。前后逐文件哈希全部一致。', '',
'| 指标 | 结果 | 范围 |','|---|---:|---|',
f'| 枚举 | {r["scan"]["enumeration_ms"]}ms | 后端枚举，不是首屏 |',
f'| 扫描/索引流水线 | {r["scan"]["pipeline_ms"]}ms | 不等于全部缩略图生成 |',
f'| 启动内部 UI 回调 P95 | {r["startup"]["internal_first_ui_p95_ms"]}ms | 20 次空窗口、暖 OS 缓存 |',
f'| WaitForInputIdle P95 | {r["startup"]["external_input_idle_p95_ms"]:.2f}ms | 不等于可见可交互首帧 |']
for s in r['scroll']:
 lines += [f'| {s["label"]} 回调 P95/P99 | {s["callback_interval_p95_ms"]:.2f}/{s["callback_interval_p99_ms"]:.2f}ms | {s["seconds"]:.2f}s；全部 4K/150% |',f'| {s["label"]} 工作集采样峰值 | {s["peak_working_set_mib"]:.2f}MiB | 250ms 采样，不保证捕获瞬时峰值 |',f'| {s["label"]} 纹理账面峰值 | {s["peak_texture_mib"]:.2f}MiB | 不是完整驱动显存计量 |']
lines += [f'| 空闲 CPU | {r["idle_cpu_percent"]:.6f}% | 预热15秒、采样至少30秒，按16线程归一化 |','',
'上述可测代理目标均达标：'+str(r['all_measured_proxy_targets_met'])+'。仍未测实际显示呈现、真实输入到 seek 新画面 P95、三次系统重启冷启动、首媒体显示延迟。未用内部回调替代这些标准。', '',
'## 源码与环境失败','',
f'独立解压原交付源码 {src["source_files"]} 文件，恢复 24 个锁定运行库，实际 cargo check --bins --locked --offline 和 cargo build --release --locked --offline 完成。独立源码 EXE SHA256：`{src["source_release_exe_sha256"]}`，与交付二进制分开保存；实际 HEIC/MOV CLI 及 2 个原生 GUI/音频/seek 病例通过。使用已有 Rust/MSVC/Cargo 离线缓存，不是干净机器零依赖构建，也不宣称字节可复现。', '',
'源码包随后仅补充工具和报告；验证器逐一核对独立构建输入与当前交付全部生产文件哈希，防止用文档更新掩盖源码变化。最终源码 ZIP 另行独立解压检查。', '',
'可执行测试集 64 项通过、3 项专用诊断忽略。此前 6 项原生测试的访问拒绝/回收拒绝记录保留；不能概括为所有启动均失败：本轮普通独立实例及 20 次启动实际成功，但单实例客户端转交和注册表写入等失败范围仍未获完整新证据。没有修改 ACL、终止用户旧窗口或将系统回收失败当作成功。', '',
'文件夹确认流程仅在专用生成目录验证撤回与取消；人工确认后的后端永久删除仅对专用生成副本测试。真实 CloudDrive 删除未执行。永久删除不具备事务性，中途 I/O 失败可能已删除部分子内容；该场景的提示与统计仍待专门验收，不能假设失败时整棵目录完整保留。', '',
'## 未完成项','',
'Win11（用户已说明暂无条件）、干净离线正常 GPU 系统完整流程、真实 iPhone 10-bit/HDR/杜比视界原片、完整物理输入与 100/200% DPI、当前构建的完整系统回收/安装及异常失败矩阵仍未完成。应用开源许可暂缓决定；测试包不是公开发行清障完成的版本。', '',
'原始证据入口：`artifacts/folders-release-recheck.json`、`folders-format-matrix.json`、`folders-user-heic-verification.json`、`folders-detail-raw.json`、`folders-source-release.json`、`folders-source-gui.json`、`folders-startup.json`、`p2-scroll-folders-final-*-process.json`、`folders-browser-idle.json`。']
(ROOT/'docs/文件夹构建格式性能与源码复验.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')
