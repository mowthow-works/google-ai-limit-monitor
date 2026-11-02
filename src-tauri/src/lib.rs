use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;
use tauri::Manager;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BucketInfo {
    pub bucket_id: String,
    pub display_name: String,
    pub window: String,
    pub remaining_fraction: f64,
    pub percentage: f64,
    pub reset_time: String,
    pub remaining_seconds: i64,
    pub remaining_formatted: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuotaGroup {
    pub display_name: String,
    pub description: String,
    pub five_hour_bucket: Option<BucketInfo>,
    pub weekly_bucket: Option<BucketInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuotaStatus {
    pub connected: bool,
    pub status_message: String,
    pub plan_name: String,
    pub prompt_credits: i64,
    pub email: String,
    pub updated_at: String,
    pub groups: Vec<QuotaGroup>,
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

fn fetch_rpc(
    token: &str,
    ports: &[u16],
    method: &str,
) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_millis(1500))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    for port in ports {
        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
            port, method
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

    Err(format!("Could not connect to {} on any port", method))
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
                plan_name: "未接続".to_string(),
                prompt_credits: 0,
                email: "".to_string(),
                updated_at,
                groups: Vec::new(),
            };
        }
    };

    // 1. GetUserStatus からプラン名とクレジットを取得
    let mut email = String::new();
    let mut plan_name = "Pro".to_string();
    let mut prompt_credits: i64 = 0;

    if let Ok(status_json) = fetch_rpc(&token, &ports, "GetUserStatus") {
        let user_status = &status_json["userStatus"];
        email = user_status["email"].as_str().unwrap_or("").to_string();
        plan_name = user_status["planStatus"]["planInfo"]["planName"]
            .as_str()
            .unwrap_or("Pro")
            .to_string();
        prompt_credits = user_status["planStatus"]["availablePromptCredits"]
            .as_i64()
            .unwrap_or(0);
    }

    // 2. RetrieveUserQuotaSummary から3グループの最新クォータ詳細を取得
    let summary_json = match fetch_rpc(&token, &ports, "RetrieveUserQuotaSummary") {
        Ok(j) => j,
        Err(err) => {
            return QuotaStatus {
                connected: false,
                status_message: format!("通信エラー: {}", err),
                plan_name: "エラー".to_string(),
                prompt_credits,
                email,
                updated_at,
                groups: Vec::new(),
            };
        }
    };

    let mut parsed_groups = Vec::new();

    if let Some(raw_groups) = summary_json["response"]["groups"].as_array() {
        for g in raw_groups {
            let display_name = g["displayName"].as_str().unwrap_or("").to_string();
            let description = g["description"].as_str().unwrap_or("").to_string();

            let mut five_hour: Option<BucketInfo> = None;
            let mut weekly: Option<BucketInfo> = None;

            if let Some(buckets) = g["buckets"].as_array() {
                for b in buckets {
                    let bucket_id = b["bucketId"].as_str().unwrap_or("").to_string();
                    let b_display = b["displayName"].as_str().unwrap_or("").to_string();
                    let window = b["window"].as_str().unwrap_or("").to_string();
                    let fraction = b["remainingFraction"].as_f64().unwrap_or(1.0);
                    let percentage = (fraction * 1000.0).round() / 10.0;
                    let reset_time_str = b["resetTime"].as_str().unwrap_or("").to_string();

                    let mut diff_seconds: i64 = 0;
                    if let Ok(reset_dt) = reset_time_str.parse::<DateTime<Utc>>() {
                        diff_seconds = (reset_dt - now_utc).num_seconds();
                    }

                    let info = BucketInfo {
                        bucket_id,
                        display_name: b_display,
                        window: window.clone(),
                        remaining_fraction: fraction,
                        percentage,
                        reset_time: reset_time_str,
                        remaining_seconds: diff_seconds,
                        remaining_formatted: format_duration(diff_seconds),
                    };

                    if window == "5h" {
                        five_hour = Some(info);
                    } else if window == "weekly" {
                        weekly = Some(info);
                    }
                }
            }

            parsed_groups.push(QuotaGroup {
                display_name,
                description,
                five_hour_bucket: five_hour,
                weekly_bucket: weekly,
            });
        }
    }

    QuotaStatus {
        connected: true,
        status_message: "正常稼働中".to_string(),
        plan_name,
        prompt_credits,
        email,
        updated_at,
        groups: parsed_groups,
    }
}

#[tauri::command]
async fn get_quota_status() -> QuotaStatus {
    tauri::async_runtime::spawn_blocking(get_quota_status_blocking)
        .await
        .unwrap_or_else(|_| {
            let now_local = Local::now();
            QuotaStatus {
                connected: false,
                status_message: "スレッドエラー".to_string(),
                plan_name: "エラー".to_string(),
                prompt_credits: 0,
                email: "".to_string(),
                updated_at: now_local.format("%H:%M:%S").to_string(),
                groups: Vec::new(),
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
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
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
