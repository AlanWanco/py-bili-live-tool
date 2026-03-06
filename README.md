# Bilibili 直播辅助工具

这是一个简单的 B 站直播间管理与开播工具，支持更新标题、分区以及人脸验证自动弹码。

## 📂 文件构成
- `bili_live_tool.py`: 核心执行脚本。
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

### 3. 安装环境
确保已安装 Python，然后运行：
```bash
pip install -r requirements.txt
```

### 4. 运行工具
```bash
python bili_live_tool.py
```

## 🛠️ 高级用法 (自动化联动)

本工具支持通过命令行参数直接启动，适合与其他监控工具（如直播间状态检测脚本）或定时任务无缝联动：

```bash
# 指定房间、分区和标题，并跳过确认直接开播
python bili_live_tool.py --room-id 123456 --area-id 33 --title "自动化联动直播" --yes

# 仅使用配置文件的默认设置静默启动
python bili_live_tool.py --yes
```

**参数说明：**
- `--room-id`: 覆盖配置文件中的房间号。
- `--area-id`: 覆盖配置文件中的分区 ID。
- `--title`: 覆盖配置文件中的直播标题。
- `-y, --yes`: **核心参数**。跳过人工 `[Y/n]` 确认环节，直接进入开播/监测流程。

## 💡 功能提示
- **永久保活 (核心增强)**：本工具集成了 OAuth2 凭据续期机制。只要按照“准备 Cookie”步骤使用 `biliup` 生成的凭据，脚本将在凭据过期前自动执行静默续期并回写文件，实现“一次登录，长效有效”。
- **人脸验证**：如果开播时触发人脸验证，终端会直接显示二维码，用 B 站 App 扫码完成后脚本会自动继续。
- **状态监测**：脚本运行时会每 30 秒监测一次推流状态，如果直播断开会提示。
- **结束直播**：在终端按下 `Ctrl + C` 即可自动下播并退出。

## 🙏 鸣谢

本工具的开发参考或集成了以下项目的优秀代码实现与设计思路：

- [biliup/biliup](https://github.com/biliup/biliup) - 工业级的凭据管理与自动续期逻辑（核心保活机制移植自其 Rust 实现）。
- [B站推流码获取工具](https://greasyfork.org/zh-CN/scripts/536798) - 极简高效的推流凭据获取方案。
