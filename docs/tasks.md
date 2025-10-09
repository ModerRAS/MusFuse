# MusFuse 进度追踪

## 媒体引擎里程碑 (M3)

| 事项 | 状态 | 说明 |
| ---- | ---- | ---- |
| Draft `DefaultFormatTranscoder` API & tests | ✅ 完成 | 依据策略定义接口，编写 WAV 直通与 FLAC 转换测试。 |
| Implement lossless passthrough & FLAC encode | ✅ 完成 | 使用 `symphonia` 解码 + `flac-codec` 编码，通过现有单元测试。 |
| Prepare cover extraction pipeline | ✅ 完成 | 内嵌封面优先，外部图像作为回退。 |
| Streaming/chunked transcoding optimization | ✅ 完成 | 默认 256KiB 块分发，带时间戳元数据。 |

## 下一步建议

- 扩充 lossy 歌曲 passthrough 测试用例（MP3/AAC）。
- 引入错误模拟测试，覆盖缺失采样率/声道等异常分支。
- 设计 CoverExtractor 行为并与 MediaEngine 集成。
