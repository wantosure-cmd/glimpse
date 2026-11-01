# 用户提供的 HEIC 验证

日期：2026-10-18。构建 `071A6799455522ECC1A37E635F6CEBF140F70BAC67190A314DB0DDB3A10F6081`。只读检查原件，浏览器测试使用工作区内专属副本；两份原件 SHA256 前后一致。图片和截图不进入源码/便携包。

| 样本 | 主图位深 | EXIF 标注 | 编码尺寸 → 显示尺寸 | 容器变换 |
|---|---|---|---|---|
| 流星雨搬运.1.heic | 亮度/色度均 8-bit | Apple / iPhone 15，Orientation=6 | 5712×4284 → 4284×5712 | irot：逆时针 270° |
| 流星雨搬运.8.heic | 亮度/色度均 8-bit | Apple / iPhone 15，Orientation=5 | 3130×2348 → 2348×3130 | 先 irot：逆时针 90°，再 imir：垂直镜像 |

EXIF 机型是文件内标签，不是拍摄来源的不可伪造认证。主图位深由当前包内 libheif 的主图句柄读取；不是从后缀、文件大小或最终 RGBA 输出位深推断。这两张均不能作为真实 10-bit 主图或 HDR MOV 的验证样本。

## 实际浏览器结果

两例都完成封面、详情适应窗口、原像素纹理请求/显示、回到列表，均无解码错误；中文文件名在实际窗口中显示正常。首次详情的截图已视觉检查，方向自然，画面未横置。测试原始数据在 `artifacts/p5-user-heic-ui.json`，每例路径由 evidence 字段给出。

另做了独立几何核对：用 libheif 的 `ignore_transformations` 解码未经旋转/镜像的编码像素，再根据容器属性顺序反算 121 个显示采样点的原坐标，与浏览器实际封面和详情帧缓冲比较。这个方法可分辨“正确镜像”与“只旋转、漏镜像”，没有只凭宽高交换判断通过。

| 样本/画面 | 正确容器变换的平均 RGB 误差 | 完全不变换 | 只旋转、不镜像 |
|---|---:|---:|---:|
| .1 封面 / 详情 | 4.81 / 4.55 | 63.65 / 63.88 | 与正确变换相同，该图无 imir |
| .8 封面 / 详情 | 6.55 / 6.52 | 41.50 / 41.64 | 49.54 / 49.17 |

误差容许缩放采样、JPEG/HEVC 解码和 ICC 转换差异，不是专业色彩精度测量。正确变换误差显著小于错误变换，证明两张 HEIC 的旋转和其中一张真实 imir 属性在封面/详情均被执行，没有再次按 EXIF 重复旋转。

## 证据与复现

- `tools/inspect_user_heic.py 文件1 文件2`：读取主图位深/尺寸、有限 EXIF、容器变换，提取关闭变换的坐标采样参考，仅复制到专属目录。
- `tools/run_format_matrix.py --manifest test-media/format-matrix-v1/user-heic-manifest.json --output artifacts/p5-user-heic-ui.json`：两例实际浏览器执行与截图校验。
- `tools/verify_user_heic.py`：原件/副本哈希、原始编码尺寸及封面/详情几何核对；结果 `artifacts/p5-user-heic-verification.json`。

原件 .1 SHA256：`5AA0B795BDF021DAE9810B9E210ADF340BBDD363E83D3E5A78ACF3FA8DCD8772`。

原件 .8 SHA256：`E03AB27CF50FA09926DD0CCB29B6BB6F1F5B5F74B217F74F9AF478690888E473`。

真实 10-bit iPhone 主图、更多旋转/镜像组合和 HDR 视频仍待补齐。这两张的成功不等于任意 HEIC 均已覆盖。
