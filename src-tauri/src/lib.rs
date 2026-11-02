use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LimitItem {
    pub label: String,
    pub model_id: String,
    pub remaining_fraction: f64,
    pub percentage: f64,
    pub reset_time: String,
    pub remaining_seconds: i64,
    pub remaining_formatted: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuotaStatus {
    pub connected: bool,
    pub status_message: String,
    pub five_hour_limit: Option<LimitItem>,
    pub weekly_limit: Option<LimitItem>,
    pub plan_name: String,
    pub prompt_credits: i64,
    pub email: String,
    pub updated_at: String,
}

#[derive(Deserialize, Debug)]
struct ProcessDiscovery {
    #[allow(dead_code)]
    pid: Option<u32>,
    token: Option<String>,
    ports: Option<Vec<u16>>,
}

fn format_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "リセット完了".to_string();
    }
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;

    if days > 0 {
        format!("{}日 {}時間", days, hours)
    } else if hours > 0 {
        format!("{}時間 {}分", hours, minutes)
    } else {
        format!("{}分", minutes)
    }
}

fn clean_model_label(raw: &str) -> String {
    // 末尾の " (Low)", " (Medium)", " (High)" 等を除去
    if let Some(idx) = raw.rfind('(') {
        let name = raw[..idx].trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    raw.trim().to_string()
}

fn discover_language_server() -> Option<(String, Vec<u16>)> {
    let script = r#"
        $proc = Get-CimInstance Win32_Process -Filter "Name like '%language_server%'" | Select-Object -First 1 ProcessId, CommandLine
        if ($proc -and $proc.CommandLine -match '--csrf_token\s+([a-zA-Z0-9\-]+)') {
            $token = $Matches[1]
            $ports = (Get-NetTCPConnection -OwningProcess $proc.ProcessId -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty LocalPort -Unique)
            [PSCustomObject]@{ pid = $proc.ProcessId; token = $token; ports = $ports } | ConvertTo-Json -Compress
        } else {
            Write-Output "{}"
        }
    "#;

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);

    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = cmd.output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    let discovery: ProcessDiscovery = serde_json::from_str(stdout.trim()).ok()?;
    let token = discovery.token?;
    let ports = discovery.ports.unwrap_or_default();

    if ports.is_empty() {
        return None;
    }

    Some((token, ports))
}

fn fetch_user_status(token: &str, ports: &[u16]) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    for port in ports {
        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
            port
        );

        let res = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", token)
            .body("{}")
            .send();

        if let Ok(resp) = res {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>() {
                    return Ok(json);
                }
            }
        }
    }

    Err("Could not connect to Language Server on any port".to_string())
}

fn get_quota_status_blocking() -> QuotaStatus {
    let now_utc = Utc::now();
    let now_local = Local::now();
    let updated_at = now_local.format("%H:%M:%S").to_string();

    let (token, ports) = match discover_language_server() {
        Some(val) => val,
        None => {
            return QuotaStatus {
                connected: false,
                status_message: "Language Server 待機中...".to_string(),
                five_hour_limit: None,
                weekly_limit: None,
                plan_name: "未接続".to_string(),
                prompt_credits: 0,
                email: "".to_string(),
                updated_at,
            };
        }
    };

    let json = match fetch_user_status(&token, &ports) {
        Ok(j) => j,
        Err(err) => {
            return QuotaStatus {
                connected: false,
                status_message: format!("通信エラー: {}", err),
                five_hour_limit: None,
                weekly_limit: None,
                plan_name: "エラー".to_string(),
                prompt_credits: 0,
                email: "".to_string(),
                updated_at,
            };
        }
    };

    let user_status = &json["userStatus"];
    let email = user_status["email"].as_str().unwrap_or("").to_string();
    let plan_name = user_status["planStatus"]["planInfo"]["planName"]
        .as_str()
        .unwrap_or("Pro")
        .to_string();
    let prompt_credits = user_status["planStatus"]["availablePromptCredits"]
        .as_i64()
        .unwrap_or(0);

    let mut five_hour_candidate: Option<LimitItem> = None;
    let mut weekly_candidate: Option<LimitItem> = None;

    if let Some(configs) = user_status["cascadeModelConfigData"]["clientModelConfigs"].as_array() {
        for config in configs {
            let raw_label = config["label"].as_str().unwrap_or("");
            let label = clean_model_label(raw_label);
            let model_id = config["modelId"].as_str().unwrap_or("").to_string();
            let quota_info = &config["quotaInfo"];

            let fraction = quota_info["remainingFraction"].as_f64().unwrap_or(1.0);
            let percentage = (fraction * 1000.0).round() / 10.0;
            let reset_time_str = quota_info["resetTime"].as_str().unwrap_or("").to_string();

            let mut diff_seconds: i64 = 0;
            if let Ok(reset_dt) = reset_time_str.parse::<DateTime<Utc>>() {
                diff_seconds = (reset_dt - now_utc).num_seconds();
            }

            let item = LimitItem {
                label,
                model_id: model_id.clone(),
                remaining_fraction: fraction,
                percentage,
                reset_time: reset_time_str,
                remaining_seconds: diff_seconds,
                remaining_formatted: format_duration(diff_seconds),
            };

            // 5時間枠（リセットが6時間以内、またはGemini Flash等の短期モデル）
            if diff_seconds > 0 && diff_seconds <= 21600 {
                if five_hour_candidate.is_none()
                    || model_id.contains("flash")
                    || item.percentage < five_hour_candidate.as_ref().unwrap().percentage
                {
                    five_hour_candidate = Some(item.clone());
                }
            } else if diff_seconds > 21600 {
                // 週間枠（リセットが6時間超〜数日後、Argon等のモデル）
                if weekly_candidate.is_none()
                    || model_id.contains("argon")
                    || item.percentage < weekly_candidate.as_ref().unwrap().percentage
                {
                    weekly_candidate = Some(item);
                }
            }
        }
    }

    QuotaStatus {
        connected: true,
        status_message: "正常稼働中".to_string(),
        five_hour_limit: five_hour_candidate,
        weekly_limit: weekly_candidate,
        plan_name,
        prompt_credits,
        email,
        updated_at,
    }
}

// メインスレッドをブロックしない非同期コマンド
#[tauri::command]
async fn get_quota_status() -> QuotaStatus {
    tauri::async_runtime::spawn_blocking(get_quota_status_blocking)
        .await
        .unwrap_or_else(|_| {
            let now_local = Local::now();
            QuotaStatus {
                connected: false,
                status_message: "スレッドエラー".to_string(),
                five_hour_limit: None,
                weekly_limit: None,
                plan_name: "エラー".to_string(),
                prompt_credits: 0,
                email: "".to_string(),
                updated_at: now_local.format("%H:%M:%S").to_string(),
            }
        })
}

#[tauri::command]
fn drag_window(window: tauri::Window) {
    let _ = window.start_dragging();
}

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_window(window: tauri::Window) {
    let _ = window.close();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_quota_status,
            drag_window,
            minimize_window,
            close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
