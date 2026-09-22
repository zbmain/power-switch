# 图标

`power-switch-master.png` 是 imagegen 生成的 1254 × 1254 PNG 母版。

设计依据：用户提供的奶油白圆角底板、浅蓝和橙色、柔和陶土立体质感。将锤子重构为浅蓝色横向切换开关，橙色按钮上包含奶油白电源符号，不包含文字、水印或其他产品标志。底色为统一奶油白，不使用伪透明棋盘格。

生成提示重点：front-facing macOS app icon, generous rounded ivory tile, soft powder-blue toggle switch, warm apricot-orange circular knob with ivory power glyph, subtle tactile clay material, soft upper-left lighting, clean silhouette, no text or watermark.

运行 `just icons` 使用 Tauri 工具从母版生成 PNG、Windows ICO 和 macOS ICNS。`src-tauri/icons` 包含 Linux PNG。`previews` 中的 16、32、128、512 像素导出用于检查小尺寸辨识度；母版保留较柔和的质感，小尺寸以蓝橙开关轮廓为主。
