# 首页截图来源

截图于2026-11-01由0.4.46原生程序的`--design-smoke`诊断入口采集，直接复制原始PNG，没有拼接或修改产品界面。程序哈希与各图哈希见[manifest.json](manifest.json)。

媒体为本次通过Python/Pillow生成的山景插画及由该插画编码的静态画面演示视频，全部位于独立生成目录；不含用户真实照片或视频。图中的目录路径为开发工作区下的演示目录，不是用户图库。

| 截图 | 内容 |
| --- | --- |
| library.png | 12张演示图片和2个演示视频的混合列表。 |
| image-detail.png | 图片详情、前后翻页、底部缩略图。 |
| video-detail.png | 视频播放、单行进度与音量控制、混合缩略图。 |
| settings.png | 外观设置及设置分类。 |
| subfolders.png | 多层目录的连续媒体列表、左侧目录树、空分支默认折叠；独立生成的本地演示目录。 |
| folder-browser.png | 导航展开后的图片/视频混排界面，用于说明挂载目录沿用的浏览布局；不表示实际连接115或SMB。 |

`subfolders.png`由同一0.4.46程序的`--browser-smoke --quit-after 4800`截图入口采集，独立运行退出码为0，原始文件为本地`target/readme-subfolders-20261101/work/artifacts/p2-list.png`，已人工查看。此运行只为采集列表截图，未执行完整浏览器回归。`folder-browser.png`直接复制本次首页第二次采集已有的`design-sidebar.png`，未修改界面或路径文字。网络支持流程图为文档中的Mermaid示意图，不是实测截图。

本次只验看截图，不作为完整功能或性能验收。诊断脚本原有场景要求与演示夹具不同：第一次执行在phase37报告`second video fixture missing`；补充第二个生成视频后，第二次执行在phase60报告`Same-position next click did not open video`。两次原始失败记录分别保存在本地`artifacts/readme-demo-native-20261101.json`和`artifacts/readme-demo-native-r2-20261101.json`，均未改写为通过。首页图片来自第二次执行已完成的截图阶段，图片已逐张人工检查；这些失败不影响对截图来源的确认，也不用于推断完整回归通过。

本次仅更新仓库文档和展示图片，既有0.4.46本地二进制交付包保持原字节；仓库文档更新不冒充同包源码刷新。
