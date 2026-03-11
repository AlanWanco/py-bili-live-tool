# Bilibili 直播辅助工具

这是一个简单的 B 站直播间管理与开播工具，支持更新标题、分区以及人脸验证自动弹码。

## 📂 文件构成
- `bili-live-tool` (可执行文件): 核心执行程序。
- `bili_config.yaml`: 存放房间号、分区及标题。
- `bili_cookie.json`: 存放登录凭证（请勿泄露给他人）。
- `bili_areas.json`: 分区信息对照表。

## 🚀 快速开始

### 1. 准备 Cookie (关键)
为了让脚本代表你进行操作，你需要获取 B 站的登录凭证：
1. 前往 [biliup/biliup Releases](https://github.com/biliup/biliup/releases/latest) 下载适合你系统的二进制文件（如 `biliupR-v1.1.29-x86_64-windows.zip`）。
2. 解压并在命令行运行：`biliup login`。
3. 按照提示扫码登录。
4. 登录成功后，同目录下会生成一个 `cookies.json`。
5. **将其重命名为 `bili_cookie.json`** 并移动到本工具所在的文件夹内。

### 2. 配置直播间
编辑 `bili_config.yaml`：
- `room_id`: 填入你的直播间号。
- `area_id`: 填入分区 ID（可在 `bili_areas.json` 查看，如 33 为单机游戏）。
- `title`: 填入你想设置的直播标题。

### 3. 运行工具
双击可执行文件，或通过命令行运行：
```bash
./bili-live-tool
```

## 🛠️ 高级用法 (自动化联动)

本工具专为自动化流媒体流水线设计，支持通过命令行参数实现无人值守联动：

```bash
# 获取 RTMP 信息后立即以 JSON 格式输出并退出
./bili-live-tool --yes --json --no-heartbeat
```

**自动化专用参数：**
- `--json`: 启用结构化 JSON 输出。在该模式下，所有状态（开播成功、心跳、人脸验证）均以单行 JSON 形式输出到 `stdout`。
  - **人脸验证时**：JSON 会包含 `url` 和 `qr_ascii` (二维码 ASCII 字符串)，方便主工具捕获并在自己的终端画出二维码。
- `--no-heartbeat`: **纯开播模式**。成功开播并输出 RTMP 信息后立即安全退出。适合由主工具接管后续推流（如 FFmpeg 或 OBS）。
- `--continuous`: 在 JSON 模式下依然保持每 30 秒输出一次状态监控 JSON。
- `--quiet`: 彻底静默非关键日志，只保留结果输出。

### 📊 JSON 输出规范

当启用 `--json` 参数时，脚本的所有输出均为单行标准 JSON 字符串，方便程序捕获解析。

#### 1. 开播成功
```json
{
  "status": "success",
  "rtmp_addr": "rtmp://...",
  "rtmp_code": "...",
  "room_id": 123456
}
```

#### 2. 需要人脸验证
```json
{
  "status": "face_auth",
  "url": "https://passport.bilibili.com/...",
  "qr_ascii": "..." 
}
```
> `qr_ascii` 包含了二维码的字符画，联动工具捕获后可直接在自己的终端打印。

#### 3. 直播心跳状态 (需开启 --continuous)
```json
{
  "status": "heartbeat",
  "duration": "00:05:30"
}
```

#### 4. 错误信息
```json
{
  "status": "error",
  "message": "错误原因说明"
}
```

## 💡 功能提示
- **永久保活 (核心增强)**：本工具集成了 OAuth2 凭据续期机制。只要按照“准备 Cookie”步骤使用 `biliup` 生成的凭据，脚本将在凭据过期前自动执行静默续期并回写文件，实现“一次登录，长效有效”。支持 Android/BiliTV 双平台自适应。
- **极速反馈**：人脸验证状态轮询间隔已优化至 **2秒**，扫码后主工具能近乎实时地感知验证成功。
- **人脸验证**：如果交互模式运行，终端会直接显示二维码；如果 JSON 模式运行，二维码字符将包含在输出对象中。
- **状态监测**：脚本运行时会每 30 秒监测一次推流状态，如果直播断开会提示。
- **结束直播**：在终端按下 `Ctrl + C` 即可自动下播并退出。

## 🙏 鸣谢

本工具的开发参考或集成了以下项目的优秀代码实现与设计思路：

- [biliup/biliup](https://github.com/biliup/biliup) - 工业级的凭据管理与自动续期逻辑（核心保活机制移植自其 Rust 实现）。
- [B站推流码获取工具](https://greasyfork.org/zh-CN/scripts/536798) - 极简高效的推流凭据获取方案。
