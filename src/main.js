const CIRCUMFERENCE = 2 * Math.PI * 36; // r=36 -> ~226.195

// DOM要素
const statusDot = document.getElementById("status-dot");
const planBadge = document.getElementById("plan-badge");
const lastUpdated = document.getElementById("last-updated");
const creditCount = document.getElementById("credit-count");

const ring5h = document.getElementById("ring-5h");
const val5h = document.getElementById("val-5h");
const reset5h = document.getElementById("reset-5h");
const model5h = document.getElementById("model-5h");

const ringWeekly = document.getElementById("ring-weekly");
const valWeekly = document.getElementById("val-weekly");
const resetWeekly = document.getElementById("reset-weekly");
const modelWeekly = document.getElementById("model-weekly");

const btnRefresh = document.getElementById("btn-refresh");
const btnMinimize = document.getElementById("btn-minimize");
const btnClose = document.getElementById("btn-close");

let fiveHourSeconds = 0;
let weeklySeconds = 0;

function getColorByPercent(pct) {
  if (pct >= 80) return "#10b981"; // エメラルドグリーン
  if (pct >= 30) return "#f59e0b"; // アンバーイエロー
  return "#ef4444"; // ネオンレッド
}

function getGlowByPercent(pct) {
  if (pct >= 80) return "rgba(16, 185, 129, 0.35)";
  if (pct >= 30) return "rgba(245, 158, 11, 0.35)";
  return "rgba(239, 68, 68, 0.4)";
}

function updateGauge(ringElem, valElem, fraction, percentage) {
  const safeFraction = Math.max(0, Math.min(1, fraction));
  const offset = CIRCUMFERENCE * (1 - safeFraction);
  const color = getColorByPercent(percentage);
  const glow = getGlowByPercent(percentage);

  ringElem.style.strokeDashoffset = offset;
  ringElem.style.stroke = color;
  ringElem.style.filter = `drop-shadow(0 0 5px ${glow})`;
  valElem.textContent = `${percentage.toFixed(0)}%`;
  valElem.style.color = color;
}

function formatDuration(seconds) {
  if (seconds <= 0) return "リセット完了";
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  if (days > 0) return `${days}日 ${hours}時間`;
  if (hours > 0) return `${hours}時間 ${minutes}分`;
  return `${minutes}分`;
}

function cleanLabel(label) {
  return (label || "").replace(/\s*\([^)]*\)$/, "").trim();
}

function getLocalTimeString() {
  const now = new Date();
  return now.toLocaleTimeString("ja-JP", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

async function fetchStatus() {
  btnRefresh.classList.add("spinning");

  try {
    const invoke = window.__TAURI__?.core?.invoke;
    if (!invoke) {
      console.warn("Tauri invoke is not available in browser mode");
      btnRefresh.classList.remove("spinning");
      return;
    }

    const data = await invoke("get_quota_status");

    if (data.connected) {
      statusDot.className = "status-dot connected";
      statusDot.title = `接続中 (${data.email})`;
      planBadge.textContent = data.plan_name;
      creditCount.textContent = (data.prompt_credits || 0).toLocaleString();
      // 日本時間（端末のローカル時刻）で表示
      lastUpdated.textContent = getLocalTimeString();

      // 5-Hour Limit
      if (data.five_hour_limit) {
        const item = data.five_hour_limit;
        fiveHourSeconds = item.remaining_seconds;
        updateGauge(ring5h, val5h, item.remaining_fraction, item.percentage);
        reset5h.textContent = `リセット: ${formatDuration(fiveHourSeconds)}`;
        model5h.textContent = cleanLabel(item.label);
      } else {
        updateGauge(ring5h, val5h, 1.0, 100);
        reset5h.textContent = "リセット: 正常";
        model5h.textContent = "枠消費なし";
      }

      // Weekly Limit
      if (data.weekly_limit) {
        const item = data.weekly_limit;
        weeklySeconds = item.remaining_seconds;
        updateGauge(ringWeekly, valWeekly, item.remaining_fraction, item.percentage);
        resetWeekly.textContent = `リセット: ${formatDuration(weeklySeconds)}`;
        modelWeekly.textContent = cleanLabel(item.label);
      } else {
        updateGauge(ringWeekly, valWeekly, 1.0, 100);
        resetWeekly.textContent = "リセット: 正常";
        modelWeekly.textContent = "枠消費なし";
      }
    } else {
      statusDot.className = "status-dot disconnected";
      statusDot.title = data.status_message;
      planBadge.textContent = "待機中";
      reset5h.textContent = "リセット: --";
      resetWeekly.textContent = "リセット: --";
      model5h.textContent = "未検出";
      modelWeekly.textContent = "未検出";
      lastUpdated.textContent = getLocalTimeString();
    }
  } catch (err) {
    console.error("fetchStatus failed:", err);
    statusDot.className = "status-dot disconnected";
  } finally {
    setTimeout(() => {
      btnRefresh.classList.remove("spinning");
    }, 400);
  }
}

// 毎秒ローカルタイマー減算
setInterval(() => {
  if (fiveHourSeconds > 0) {
    fiveHourSeconds--;
    reset5h.textContent = `リセット: ${formatDuration(fiveHourSeconds)}`;
  }
  if (weeklySeconds > 0) {
    weeklySeconds--;
    resetWeekly.textContent = `リセット: ${formatDuration(weeklySeconds)}`;
  }
}, 1000);

// タイトルバー領域（Google AI Limits / PROバッジ周辺）のドラッグ移動ハンドラー
const gadgetHeader = document.querySelector(".gadget-header");
if (gadgetHeader) {
  gadgetHeader.addEventListener("mousedown", async (e) => {
    // 操作ボタン群のクリック時はドラッグをトリガーしない
    if (e.target.closest("button") || e.target.closest(".icon-btn") || e.target.closest(".window-controls")) {
      return;
    }
    // 左クリック時のみドラッグ開始
    if (e.button === 0) {
      const invoke = window.__TAURI__?.core?.invoke;
      if (invoke) {
        try {
          await invoke("drag_window");
        } catch (err) {
          console.error("drag_window error:", err);
        }
      }
    }
  });
}

// ウィンドウ制御ボタン
btnRefresh.addEventListener("click", (e) => {
  e.stopPropagation();
  fetchStatus();
});

btnMinimize.addEventListener("click", async (e) => {
  e.stopPropagation();
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) await invoke("minimize_window");
});

btnClose.addEventListener("click", async (e) => {
  e.stopPropagation();
  const invoke = window.__TAURI__?.core?.invoke;
  if (invoke) await invoke("close_window");
});

// 初期読み込みと10分ポーリング（手動更新も可能）
window.addEventListener("DOMContentLoaded", () => {
  fetchStatus();
  setInterval(fetchStatus, 10 * 60 * 1000);
});
