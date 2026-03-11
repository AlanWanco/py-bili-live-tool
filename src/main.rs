use chrono::Local;
use clap::Parser;
use qrcode::QrCode;
use qrcode::render::unicode;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(version, about = "Bilibili 直播辅助工具")]
struct Args {
    #[arg(long, help = "直播间号 (覆盖配置)")]
    room_id: Option<u64>,

    #[arg(long, help = "分区 ID (覆盖配置)")]
    area_id: Option<u64>,

    #[arg(long, help = "直播标题 (覆盖配置)")]
    title: Option<String>,

    #[arg(short, long, help = "跳过确认直接开播")]
    yes: bool,

    #[arg(long, help = "启用 JSON 输出模式")]
    json: bool,

    #[arg(long, help = "获取推流码后立即退出")]
    no_heartbeat: bool,

    #[arg(long, help = "JSON模式下持续输出心跳")]
    continuous: bool,

    #[arg(long, help = "静默模式，仅输出关键结果")]
    quiet: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    room_id: Option<u64>,
    area_id: Option<u64>,
    title: Option<String>,
}

struct BiliLiveTool {
    cookie_file_path: PathBuf,
    is_json: bool,
    quiet: bool,
    raw_data: Value,
    cookies: HashMap<String, String>,
    token_info: Value,
    csrf: String,
    client: Client,
}

impl BiliLiveTool {
    fn new(cookie_file_path: PathBuf, is_json: bool, quiet: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let mut tool = Self {
            cookie_file_path,
            is_json,
            quiet,
            raw_data: json!({}),
            cookies: HashMap::new(),
            token_info: json!({}),
            csrf: String::new(),
            client: Client::builder().cookie_store(true).build()?,
        };
        tool.load_cookies()?;
        Ok(tool)
    }

    fn _emit(&self, status: &str, message: Option<&str>, kwargs: Option<Value>) {
        if self.is_json {
            let mut out = json!({ "status": status });
            if let Some(msg) = message {
                out["message"] = json!(msg);
            }
            if let Some(Value::Object(map)) = kwargs {
                for (k, v) in map {
                    out[k] = v;
                }
            }
            println!("{}", serde_json::to_string(&out).unwrap());
        } else {
            let msg = message.unwrap_or("");
            if status == "error" {
                eprintln!("{} [ERROR] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
            } else if status == "success" {
                println!("{} [INFO] 🚀 {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
            } else if status == "face_auth" {
                println!("{} [WARNING] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
            } else if !self.quiet {
                println!("{} [INFO] {}", Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
            }
        }
    }

    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36"));
        headers.insert(REFERER, HeaderValue::from_static("https://www.bilibili.com/"));
        
        let mut cookie_str = String::new();
        for (k, v) in &self.cookies {
            cookie_str.push_str(&format!("{}={}; ", k, v));
        }
        if let Ok(v) = HeaderValue::from_str(&cookie_str) {
            headers.insert(COOKIE, v);
        }
        headers
    }

    fn load_cookies(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.cookie_file_path.exists() {
            return Err(format!("找不到 Cookie 文件: {:?}", self.cookie_file_path).into());
        }
        let content = fs::read_to_string(&self.cookie_file_path)?;
        
        if let Ok(data) = serde_json::from_str::<Value>(&content) {
            self.raw_data = data;
            if let Some(cookies_arr) = self.raw_data.get("cookie_info").and_then(|info| info.get("cookies")).and_then(|c| c.as_array()) {
                for item in cookies_arr {
                    if let (Some(name), Some(value)) = (item.get("name").and_then(|n| n.as_str()), item.get("value").and_then(|v| v.as_str())) {
                        self.cookies.insert(name.to_string(), value.to_string());
                    }
                }
            }
            if let Some(token) = self.raw_data.get("token_info") {
                self.token_info = token.clone();
            }
        } else {
            for item in content.trim().split("; ") {
                if let Some((k, v)) = item.split_once('=') {
                    self.cookies.insert(k.to_string(), v.to_string());
                }
            }
        }

        if let Some(jct) = self.cookies.get("bili_jct") {
            self.csrf = jct.clone();
        }

        Ok(())
    }

    fn save_cookies(&self) {
        if let Ok(json_str) = serde_json::to_string_pretty(&self.raw_data) {
            let _ = fs::write(&self.cookie_file_path, json_str);
            if !self.quiet {
                println!("{} [INFO] 已更新本地 Cookie 文件", Local::now().format("%Y-%m-%d %H:%M:%S"));
            }
        }
    }

    fn sign(&self, params: &mut Vec<(&str, String)>, app_sec: &str) -> String {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        params.push(("ts", ts));
        params.sort_by(|a, b| a.0.cmp(&b.0));

        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter().map(|(k, v)| (*k, v)))
            .finish();

        let sign_str = format!("{}{}", query, app_sec);
        use md5::{Md5, Digest};
        let mut hasher = Md5::new();
        hasher.update(sign_str.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    async fn check_and_refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let access_token = match self.token_info.get("access_token").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => return Ok(()),
        };

        let platform = self.raw_data.get("platform").and_then(|p| p.as_str()).unwrap_or("Android");
        let (app_key, app_sec) = if platform == "BiliTV" {
            ("4409e2ce8ffd12b8", "59b43e04ad6965f34319062b478f83dd")
        } else {
            ("783bbb7264451d82", "2653583c8873dea268ab9386918b1d65")
        };

        let mut params = vec![
            ("access_key", access_token.to_string()),
            ("appkey", app_key.to_string()),
            ("actionKey", "appkey".to_string()),
        ];
        let sign = self.sign(&mut params, app_sec);
        params.push(("sign", sign));

        let res = self.client.get("https://passport.bilibili.com/x/passport-login/oauth2/info")
            .query(&params)
            .headers(self.build_headers())
            .send()
            .await?;
        
        if res.status().is_success() {
            let json_res: Value = res.json().await?;
            if json_res["code"] == 0 && json_res["data"]["refresh"] == true {
                self._emit("info", Some("[*] 检测到凭据需要续期，正在尝试自动刷新..."), None);

                let refresh_token = self.token_info.get("refresh_token").and_then(|t| t.as_str()).unwrap_or("");
                let mut refresh_params = vec![
                    ("access_key", access_token.to_string()),
                    ("refresh_token", refresh_token.to_string()),
                    ("appkey", app_key.to_string()),
                    ("actionKey", "appkey".to_string()),
                ];
                let refresh_sign = self.sign(&mut refresh_params, app_sec);
                refresh_params.push(("sign", refresh_sign));

                let refresh_res = self.client.post("https://passport.bilibili.com/x/passport-login/oauth2/refresh_token")
                    .form(&refresh_params)
                    .headers(self.build_headers())
                    .send()
                    .await?;
                
                if refresh_res.status().is_success() {
                    let refresh_json: Value = refresh_res.json().await?;
                    if refresh_json["code"] == 0 {
                        let new_data = &refresh_json["data"];
                        if new_data.get("cookie_info").is_some() {
                            self.raw_data["cookie_info"] = new_data["cookie_info"].clone();
                            if let Some(cookies_arr) = new_data["cookie_info"]["cookies"].as_array() {
                                for item in cookies_arr {
                                    if let (Some(name), Some(value)) = (item["name"].as_str(), item["value"].as_str()) {
                                        self.cookies.insert(name.to_string(), value.to_string());
                                    }
                                }
                            }
                        }
                        if new_data.get("token_info").is_some() {
                            self.raw_data["token_info"] = new_data["token_info"].clone();
                            self.token_info = new_data["token_info"].clone();
                        }
                        if let Some(jct) = self.cookies.get("bili_jct") {
                            self.csrf = jct.clone();
                        }
                        self.save_cookies();
                        self._emit("info", Some("✅ 凭据续期成功！"), None);
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_login(&self) -> bool {
        let url = "https://api.bilibili.com/x/web-interface/nav";
        match self.client.get(url).headers(self.build_headers()).send().await {
            Ok(res) => {
                if let Ok(json) = res.json::<Value>().await {
                    if json["code"] == 0 {
                        if json["data"]["isLogin"] == true {
                            if !self.quiet && !self.is_json {
                                let uname = json["data"]["uname"].as_str().unwrap_or("");
                                let mid = json["data"]["mid"].as_u64().unwrap_or(0);
                                println!("{} [INFO] ✅ 登录成功: {} (MID: {})", Local::now().format("%Y-%m-%d %H:%M:%S"), uname, mid);
                            }
                            return true;
                        }
                    }
                    self._emit("error", Some(&format!("❌ Cookie 已过期或无效: {}", json["message"].as_str().unwrap_or("未登录"))), None);
                }
            }
            Err(e) => self._emit("error", Some(&format!("检查登录状态时发生网络异常: {}", e)), None),
        }
        false
    }

    async fn update_room_info(&self, room_id: u64, title: Option<&str>, area_id: Option<u64>) -> Result<Value, Box<dyn std::error::Error>> {
        let url = "https://api.live.bilibili.com/room/v1/Room/update";
        let mut form = HashMap::new();
        form.insert("room_id", room_id.to_string());
        form.insert("csrf_token", self.csrf.clone());
        form.insert("csrf", self.csrf.clone());
        if let Some(t) = title {
            form.insert("title", t.to_string());
        }
        if let Some(a) = area_id {
            form.insert("area_id", a.to_string());
        }

        let res = self.client.post(url)
            .form(&form)
            .headers(self.build_headers())
            .send()
            .await?;
        Ok(res.json().await?)
    }

    async fn start_live(&self, room_id: u64, area_id: u64) -> Result<Value, Box<dyn std::error::Error>> {
        let url = "https://api.live.bilibili.com/room/v1/Room/startLive";
        let form = vec![
            ("room_id", room_id.to_string()),
            ("platform", "pc_link".to_string()),
            ("area_v2", area_id.to_string()),
            ("backup_stream", "0".to_string()),
            ("csrf_token", self.csrf.clone()),
            ("csrf", self.csrf.clone()),
        ];
        let res = self.client.post(url)
            .form(&form)
            .headers(self.build_headers())
            .send()
            .await?;
        let json_res: Value = res.json().await?;
        
        if json_res["code"] == 0 {
            let addr = json_res["data"]["rtmp"]["addr"].as_str().unwrap_or("");
            let code = json_res["data"]["rtmp"]["code"].as_str().unwrap_or("");
            Ok(json!({
                "status": "success",
                "rtmp_addr": addr,
                "rtmp_code": code
            }))
        } else if json_res["code"] == 60024 {
            let data = &json_res["data"];
            let url = data.get("qr").or(data.get("url")).or(data.get("face_auth_url")).and_then(|u| u.as_str()).unwrap_or("");
            Ok(json!({
                "status": "face_auth",
                "message": "需要人脸验证",
                "url": url
            }))
        } else {
            let msg = json_res["message"].as_str().unwrap_or("未知错误");
            Ok(json!({
                "status": "error",
                "message": msg,
                "code": json_res["code"]
            }))
        }
    }

    async fn get_live_status(&self, room_id: u64) -> i64 {
        let url = format!("https://api.live.bilibili.com/room/v1/Room/get_info?room_id={}", room_id);
        if let Ok(res) = self.client.get(&url).headers(self.build_headers()).send().await {
            if let Ok(json) = res.json::<Value>().await {
                if json["code"] == 0 {
                    return json["data"]["live_status"].as_i64().unwrap_or(-1);
                }
            }
        }
        -1
    }

    async fn stop_live(&self, room_id: u64) {
        let url = "https://api.live.bilibili.com/room/v1/Room/stopLive";
        let form = vec![
            ("room_id", room_id.to_string()),
            ("csrf_token", self.csrf.clone()),
            ("csrf", self.csrf.clone()),
        ];
        let _ = self.client.post(url).form(&form).headers(self.build_headers()).send().await;
    }

    async fn check_face_auth_status(&self, room_id: u64) -> bool {
        let url = "https://api.live.bilibili.com/xlive/app-blink/v1/preLive/IsUserIdentifiedByFaceAuth";
        let form = vec![
            ("room_id", room_id.to_string()),
            ("face_auth_code", "60024".to_string()),
            ("csrf_token", self.csrf.clone()),
            ("csrf", self.csrf.clone()),
        ];
        if let Ok(res) = self.client.post(url).form(&form).headers(self.build_headers()).send().await {
            if let Ok(json) = res.json::<Value>().await {
                if json["code"] == 0 {
                    return json["data"]["is_identified"].as_bool().unwrap_or(false);
                }
            }
        }
        false
    }

    async fn run_live(&mut self, room_id: u64, area_id: u64, title: String, no_heartbeat: bool, continuous: bool) {
        let _ = self.check_and_refresh().await;
        if !self.check_login().await {
            return;
        }

        let _ = self.update_room_info(room_id, Some(&title), Some(area_id)).await;

        loop {
            match self.start_live(room_id, area_id).await {
                Ok(live_res) => {
                    let status = live_res["status"].as_str().unwrap_or("");
                    if status == "success" {
                        let rtmp_addr = live_res["rtmp_addr"].as_str().unwrap_or("");
                        let rtmp_code = live_res["rtmp_code"].as_str().unwrap_or("");
                        
                        self._emit("success", Some("开播成功！"), Some(json!({
                            "rtmp_addr": rtmp_addr,
                            "rtmp_code": rtmp_code,
                            "room_id": room_id
                        })));

                        if !self.is_json {
                            println!("\n推流地址: {}\n推流码: {}\n", rtmp_addr, rtmp_code);
                        }

                        if no_heartbeat {
                            sleep(Duration::from_millis(500)).await;
                            return;
                        }

                        let start_time = SystemTime::now();
                        let mut last_refresh_check = SystemTime::now();

                        loop {
                            sleep(Duration::from_secs(30)).await;
                            let status = self.get_live_status(room_id).await;
                            
                            let elapsed = start_time.elapsed().unwrap_or(Duration::from_secs(0)).as_secs();
                            let h = elapsed / 3600;
                            let m = (elapsed % 3600) / 60;
                            let s = elapsed % 60;
                            let duration_str = format!("{:02}:{:02}:{:02}", h, m, s);

                            if last_refresh_check.elapsed().unwrap_or(Duration::from_secs(0)).as_secs() > 14400 {
                                let _ = self.check_and_refresh().await;
                                last_refresh_check = SystemTime::now();
                            }

                            if status == 1 {
                                if continuous || !self.is_json {
                                    self._emit("heartbeat", Some(&format!("心跳正常 - 已直播: {}", duration_str)), Some(json!({
                                        "duration": duration_str
                                    })));
                                }
                            } else if status == 0 {
                                self._emit("error", Some("⚠️ 直播已断开"), None);
                                break;
                            }
                        }
                        break;
                    } else if status == "face_auth" {
                        let auth_url = live_res["url"].as_str().unwrap_or("");
                        let mut qr_ascii = String::new();
                        
                        if let Ok(code) = QrCode::new(auth_url) {
                            qr_ascii = code.render::<unicode::Dense1x2>()
                                .dark_color(unicode::Dense1x2::Light)
                                .light_color(unicode::Dense1x2::Dark)
                                .build();
                        }

                        self._emit("face_auth", Some("需要人脸验证"), Some(json!({
                            "url": auth_url,
                            "qr_ascii": qr_ascii
                        })));

                        if !self.is_json && !qr_ascii.is_empty() {
                            println!("{}", qr_ascii);
                        } else if !self.is_json {
                            println!("验证链接: {}", auth_url);
                        }

                        let mut verified = false;
                        for _ in 0..300 {
                            if self.check_face_auth_status(room_id).await {
                                self._emit("info", Some("✅ 人脸验证成功！"), None);
                                verified = true;
                                break;
                            }
                            sleep(Duration::from_secs(2)).await;
                        }

                        if !verified {
                            self._emit("error", Some("人脸验证超时"), None);
                            break;
                        }
                    } else {
                        let msg = live_res["message"].as_str().unwrap_or("");
                        self._emit("error", Some(&format!("开播失败: {}", msg)), None);
                        break;
                    }
                }
                Err(e) => {
                    self._emit("error", Some(&format!("网络异常: {}", e)), None);
                    break;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let base_dir = env::current_exe().unwrap_or_else(|_| PathBuf::from(".")).parent().unwrap().to_path_buf();
    
    // In dev mode (cargo run), the binary is in target/debug, so we should look for configs in the workspace root too
    let mut cookie_path = base_dir.join("bili_cookie.json");
    let mut config_path = base_dir.join("bili_config.yaml");

    if !cookie_path.exists() && std::path::Path::new("bili_cookie.json").exists() {
        cookie_path = PathBuf::from("bili_cookie.json");
    }
    if !config_path.exists() && std::path::Path::new("bili_config.yaml").exists() {
        config_path = PathBuf::from("bili_config.yaml");
    }

    let config = if let Ok(c) = fs::read_to_string(&config_path) {
        serde_yaml::from_str::<Config>(&c).unwrap_or(Config { room_id: None, area_id: None, title: None })
    } else {
        Config { room_id: None, area_id: None, title: None }
    };

    let room_id = args.room_id.or(config.room_id).expect("缺少 room_id，请通过 --room-id 提供或在 bili_config.yaml 中配置");
    let area_id = args.area_id.or(config.area_id).expect("缺少 area_id");
    let title = args.title.or(config.title).expect("缺少 title");

    if !args.yes && !args.json {
        println!("\n--- 开播确认 ---\n房间: {}\n标题: {}\n分区: {}", room_id, title, area_id);
        print!("\n确认开播？[Y/n]: ");
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim().to_lowercase();
        if input != "" && input != "y" && input != "yes" {
            println!("已取消。");
            return;
        }
    }

    let mut tool = match BiliLiveTool::new(cookie_path, args.json, args.quiet) {
        Ok(t) => t,
        Err(e) => {
            if args.json {
                println!("{}", json!({"status": "error", "message": e.to_string()}));
            } else {
                eprintln!("运行异常: {}", e);
            }
            return;
        }
    };

    // Handle Ctrl-C gracefully
    let csrf_clone = tool.csrf.clone();
    let client_clone = tool.client.clone();
    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            let url = "https://api.live.bilibili.com/room/v1/Room/stopLive";
            let form = vec![
                ("room_id", room_id.to_string()),
                ("csrf_token", csrf_clone.clone()),
                ("csrf", csrf_clone),
            ];
            let _ = client_clone.post(url).form(&form).send().await;
            println!("✅ 已下播");
            std::process::exit(0);
        }
    });

    tool.run_live(room_id, area_id, title, args.no_heartbeat, args.continuous).await;
}