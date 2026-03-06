import requests
import json
import time
import os
import yaml
import logging
import hashlib
import urllib.parse
import sys
import io
from datetime import datetime

# 配置日志格式
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
    datefmt="%Y-%m-%d %H:%M:%S",
)
logger = logging.getLogger("BiliLive")

# B站移动端 AppKey 映射 (移植自 biliup-rs)
PLATFORM_KEYS = {
    "Android": ("783bbb7264451d82", "2653583c8873dea268ab9386918b1d65"),
    "BiliTV": ("4409e2ce8ffd12b8", "59b43e04ad6965f34319062b478f83dd"),
}


class BiliLiveTool:
    def __init__(self, cookie_file_path, is_json=False, quiet=False):
        self.cookie_file_path = cookie_file_path
        self.is_json = is_json
        self.quiet = quiet
        self.session = requests.Session()
        self.raw_data = {}  # 存储完整的登录信息
        self.cookies = {}
        self.token_info = {}
        self.csrf = ""
        self.load_cookies()

        if self.quiet:
            logger.setLevel(logging.WARNING)

    def _emit(self, status, message=None, **kwargs):
        """统一输出函数，支持纯文本和 JSON"""
        if self.is_json:
            out = {"status": status}
            if message:
                out["message"] = message
            out.update(kwargs)
            # 确保 JSON 输出是单行且不带前缀，方便主工具捕获
            print(json.dumps(out, ensure_ascii=False), flush=True)
        else:
            if status == "error":
                logger.error(message)
            elif status == "success":
                logger.info(f"🚀 {message}")
            elif status == "face_auth":
                logger.warning(message)
            elif not self.quiet:
                logger.info(message)

    def _sign(self, params, app_sec):
        """B站 API 签名算法"""
        params["ts"] = int(time.time())
        items = sorted(params.items())
        query = urllib.parse.urlencode(items)
        sign_str = query + app_sec
        return hashlib.md5(sign_str.encode()).hexdigest()

    def load_cookies(self):
        """从本地文件加载 Cookie 和 Token"""
        if not os.path.exists(self.cookie_file_path):
            raise FileNotFoundError(f"找不到 Cookie 文件: {self.cookie_file_path}")

        with open(self.cookie_file_path, "r", encoding="utf-8") as f:
            try:
                self.raw_data = json.load(f)
                if "cookie_info" in self.raw_data:
                    for item in self.raw_data["cookie_info"].get("cookies", []):
                        self.cookies[item["name"]] = item["value"]
                if "token_info" in self.raw_data:
                    self.token_info = self.raw_data["token_info"]
            except json.JSONDecodeError:
                f.seek(0)
                content = f.read()
                for item in content.strip().split("; "):
                    if "=" in item:
                        k, v = item.split("=", 1)
                        self.cookies[k] = v

        self.session.cookies.update(self.cookies)
        self.csrf = self.cookies.get("bili_jct", "")
        self.session.headers.update(
            {
                "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
                "Referer": "https://www.bilibili.com/",
            }
        )

    def save_cookies(self):
        """将更新后的凭据保存回文件"""
        with open(self.cookie_file_path, "w", encoding="utf-8") as f:
            json.dump(self.raw_data, f, indent=2, ensure_ascii=False)
        if not self.quiet:
            logger.info("已更新本地 Cookie 文件")

    def check_and_refresh(self):
        """检查并自动刷新 Token (移植自 biliup-rs)"""
        if not self.token_info.get("access_token"):
            return

        platform = self.raw_data.get("platform", "Android")
        app_key, app_sec = PLATFORM_KEYS.get(platform, PLATFORM_KEYS["Android"])

        params = {
            "access_key": self.token_info["access_token"],
            "appkey": app_key,
            "actionKey": "appkey",
        }
        params["sign"] = self._sign(params, app_sec)

        try:
            response = self.session.get(
                "https://passport.bilibili.com/x/passport-login/oauth2/info",
                params=params,
                timeout=10,
            )
            if response.status_code != 200:
                return

            res = response.json()
            if res.get("code") == 0 and res["data"].get("refresh"):
                self._emit("info", "[*] 检测到凭据需要续期，正在尝试自动刷新...")

                refresh_params = {
                    "access_key": self.token_info["access_token"],
                    "refresh_token": self.token_info["refresh_token"],
                    "appkey": app_key,
                    "actionKey": "appkey",
                }
                refresh_params["sign"] = self._sign(refresh_params, app_sec)

                refresh_response = self.session.post(
                    "https://passport.bilibili.com/x/passport-login/oauth2/refresh_token",
                    data=refresh_params,
                    timeout=15,
                )

                if refresh_response.status_code == 200:
                    refresh_res = refresh_response.json()
                    if refresh_res.get("code") == 0:
                        new_data = refresh_res["data"]
                        if "cookie_info" in new_data:
                            self.raw_data["cookie_info"] = new_data["cookie_info"]
                            for item in new_data["cookie_info"].get("cookies", []):
                                self.cookies[item["name"]] = item["value"]
                        if "token_info" in new_data:
                            self.raw_data["token_info"] = new_data["token_info"]
                            self.token_info = new_data["token_info"]

                        self.session.cookies.update(self.cookies)
                        self.csrf = self.cookies.get("bili_jct", "")
                        self.save_cookies()
                        self._emit("info", "✅ 凭据续期成功！")
        except Exception as e:
            self._emit("error", f"续期过程发生异常: {e}")

    def check_login(self):
        """验证当前 Cookie 是否有效"""
        url = "https://api.bilibili.com/x/web-interface/nav"
        try:
            res = self.session.get(url).json()
            if res["code"] == 0:
                data = res.get("data", {})
                if data.get("isLogin"):
                    if not self.quiet:
                        logger.info(
                            f"✅ 登录成功: {data.get('uname')} (MID: {data.get('mid')})"
                        )
                    return True
            self._emit(
                "error", f"❌ Cookie 已过期或无效: {res.get('message', '未登录')}"
            )
        except Exception as e:
            self._emit("error", f"检查登录状态时发生网络异常: {e}")
        return False

    def start_live(self, room_id, area_id):
        """开始直播并获取推流码"""
        url = "https://api.live.bilibili.com/room/v1/Room/startLive"
        data = {
            "room_id": room_id,
            "platform": "pc_link",
            "area_v2": area_id,
            "backup_stream": 0,
            "csrf_token": self.csrf,
            "csrf": self.csrf,
        }
        res = self.session.post(url, data=data).json()

        if res["code"] == 0:
            return {
                "status": "success",
                "rtmp_addr": res["data"]["rtmp"]["addr"],
                "rtmp_code": res["data"]["rtmp"]["code"],
            }
        elif res["code"] == 60024:
            data = res.get("data", {})
            return {
                "status": "face_auth",
                "message": "需要人脸验证",
                "url": data.get("qr") or data.get("url") or data.get("face_auth_url"),
            }
        return {
            "status": "error",
            "message": res.get("message", "未知错误"),
            "code": res["code"],
        }

    def check_face_auth_status(self, room_id):
        """检查人脸验证状态"""
        url = "https://api.live.bilibili.com/xlive/app-blink/v1/preLive/IsUserIdentifiedByFaceAuth"
        data = {
            "room_id": room_id,
            "face_auth_code": "60024",
            "csrf_token": self.csrf,
            "csrf": self.csrf,
        }
        try:
            res = self.session.post(url, data=data).json()
            if res["code"] == 0:
                return res["data"].get("is_identified", False)
        except:
            pass
        return False

    def stop_live(self, room_id):
        """停止直播"""
        url = "https://api.live.bilibili.com/room/v1/Room/stopLive"
        data = {"room_id": room_id, "csrf_token": self.csrf, "csrf": self.csrf}
        return self.session.post(url, data=data).json()

    def update_room_info(self, room_id, title=None, area_id=None):
        """修改房间标题或分区"""
        url = "https://api.live.bilibili.com/room/v1/Room/update"
        data = {"room_id": room_id, "csrf_token": self.csrf, "csrf": self.csrf}
        if title:
            data["title"] = title
        if area_id:
            data["area_id"] = area_id
        return self.session.post(url, data=data).json()

    def get_live_status(self, room_id):
        """获取当前直播状态"""
        url = f"https://api.live.bilibili.com/room/v1/Room/get_info?room_id={room_id}"
        try:
            res = self.session.get(url).json()
            if res["code"] == 0:
                return res["data"]["live_status"]
        except:
            pass
        return -1

    def run_live(self, room_id, area_id, title, no_heartbeat=False, continuous=False):
        # 0. 验证登录 & 检查续期
        self.check_and_refresh()
        if not self.check_login():
            return

        # 1. 更新房间信息
        self.update_room_info(room_id, title=title, area_id=area_id)

        # 2. 尝试开播
        while True:
            live_res = self.start_live(room_id, area_id)

            if live_res["status"] == "success":
                # 输出结果
                self._emit(
                    "success",
                    "开播成功！",
                    rtmp_addr=live_res["rtmp_addr"],
                    rtmp_code=live_res["rtmp_code"],
                    room_id=room_id,
                )

                if not self.is_json:
                    print(
                        f"\n推流地址: {live_res['rtmp_addr']}\n推流码: {live_res['rtmp_code']}\n"
                    )

                if no_heartbeat:
                    time.sleep(0.5)  # 给主工具留点解析时间
                    return

                start_time = datetime.now()
                last_refresh_check = time.time()
                try:
                    while True:
                        time.sleep(30)
                        status = self.get_live_status(room_id)
                        duration = str(datetime.now() - start_time).split(".")[0]

                        # 续期检查
                        if time.time() - last_refresh_check > 14400:
                            self.check_and_refresh()
                            last_refresh_check = time.time()

                        if status == 1:
                            if continuous or not self.is_json:
                                self._emit(
                                    "heartbeat",
                                    f"心跳正常 - 已直播: {duration}",
                                    duration=duration,
                                )
                        elif status == 0:
                            self._emit("error", "⚠️ 直播已断开")
                            break
                    break
                except KeyboardInterrupt:
                    self.stop_live(room_id)
                    self._emit("info", "✅ 已下播")
                    break

            elif live_res["status"] == "face_auth":
                auth_url = live_res["url"]
                qr_ascii = ""
                try:
                    import qrcode

                    qr = qrcode.QRCode()
                    qr.add_data(auth_url)
                    f = io.StringIO()
                    qr.print_ascii(out=f, invert=True)
                    qr_ascii = f.getvalue()
                except:
                    pass

                # 输出验证信息
                self._emit("face_auth", "需要人脸验证", url=auth_url, qr_ascii=qr_ascii)
                if not self.is_json and qr_ascii:
                    print(qr_ascii)
                elif not self.is_json:
                    print(f"验证链接: {auth_url}")

                # 快速轮询验证状态 (2s 间隔)
                verified = False
                for _ in range(300):  # 10分钟
                    if self.check_face_auth_status(room_id):
                        self._emit("info", "✅ 人脸验证成功！")
                        verified = True
                        break
                    time.sleep(2)
                if not verified:
                    self._emit("error", "人脸验证超时")
                    break
            else:
                self._emit("error", f"开播失败: {live_res['message']}")
                break


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Bilibili 直播辅助工具")
    parser.add_argument("--room-id", help="直播间号 (覆盖配置)")
    parser.add_argument("--area-id", help="分区 ID (覆盖配置)")
    parser.add_argument("--title", help="直播标题 (覆盖配置)")
    parser.add_argument("-y", "--yes", action="store_true", help="跳过确认直接开播")
    parser.add_argument("--json", action="store_true", help="启用 JSON 输出模式")
    parser.add_argument(
        "--no-heartbeat", action="store_true", help="获取推流码后立即退出"
    )
    parser.add_argument(
        "--continuous", action="store_true", help="JSON模式下持续输出心跳"
    )
    parser.add_argument("--quiet", action="store_true", help="静默模式，仅输出关键结果")
    args = parser.parse_args()

    # 路径配置
    if os.path.exists("/Users/alanwanco/Workspace/code-repository/local_settings"):
        BASE_DIR = os.path.dirname(os.path.abspath(__file__))
    else:
        BASE_DIR = os.path.dirname(os.path.abspath(__file__))

    COOKIE_PATH = os.path.join(BASE_DIR, "bili_cookie.json")
    CONFIG_PATH = os.path.join(BASE_DIR, "bili_config.yaml")

    try:
        tool = BiliLiveTool(COOKIE_PATH, is_json=args.json, quiet=args.quiet)
        with open(CONFIG_PATH, "r", encoding="utf-8") as f:
            config = yaml.safe_load(f)

        room_id = args.room_id or config.get("room_id")
        area_id = args.area_id or config.get("area_id")
        title = args.title or config.get("title")

        if not args.yes and not args.json:
            print(
                f"\n--- 开播确认 ---\n房间: {room_id}\n标题: {title}\n分区: {area_id}"
            )
            if input("\n确认开播？[Y/n]: ").lower() not in ["", "y", "yes"]:
                print("已取消。")
                exit()

        tool.run_live(
            room_id,
            area_id,
            title,
            no_heartbeat=args.no_heartbeat,
            continuous=args.continuous,
        )
    except KeyboardInterrupt:
        pass
    except Exception as e:
        if args.json:
            print(json.dumps({"status": "error", "message": str(e)}))
        else:
            logger.exception(f"运行异常: {e}")
