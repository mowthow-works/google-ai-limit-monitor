const CIRCUMFERENCE = 2 * Math.PI * 35; // r=35 -> ~219.911

// DOM要素
const statusDot = document.getElementById("status-dot");
const planBadge = document.getElementById("plan-badge");
const lastUpdated = document.getElementById("last-updated");
const creditCount = document.getElementById("credit-count");

const card5h = document.getElementById("card-5h");
const cardNo5h = document.getElementById("card-no-5h");
const ring5h = document.getElementById("ring-5h");
const val5h = document.getElementById("val-5h");
const reset5h = document.getElementById("reset-5h");
const model5h = document.getElementById("model-5h");

const cardWeekly = document.getElementById("card-weekly");
const ringWeekly = document.getElementById("ring-weekly");
const valWeekly = document.getElementById("val-weekly");
const resetWeekly = document.getElementById("reset-weekly");
const modelWeekly = document.getElementById("model-weekly");

const modelBadges = document.getElementById("model-badges");
const modelCount = document.getElementById("model-count");

const btnRefresh = document.getElementById("btn-refresh");
const btnMinimize = document.getElementById("btn-minimize");
const btnClose = document.getElementById("btn-close");

const tabButtons = document.querySelectorAll(".tab-btn");

let currentGroup = "gemini"; // "gemini" | "argon" | "claude"
let cachedData = null;
let fiveHourSeconds = 0;
let weeklySeconds = 0;
let demoMode = false;

const DEMO_MODELS = [
  { label: "Gemini 4 Argon (Medium)", model_id: "gemini-4-argon-medium", percentage: 18, is_recommended: true },
  { label: "Gemini 4 Argon (High)", model_id: "gemini-4-argon-high", percentage: 18, is_recommended: true },
  { label: "Gemini 4 Argon (Low)", model_id: "gemini-4-argon-low", percentage: 18, is_recommended: false },
  { label: "Gemini 3.8 Flash (Medium)", model_id: "gemini-3-8-flash-medium", percentage: 45, is_recommended: true },
  { label: "Gemini 3.8 Flash (Low)", model_id: "gemini-3-8-flash-low", percentage: 45, is_recommended: false },
  { label: "Gemini 3.8 Pro (High)", model_id: "gemini-3-8-pro-high", percentage: 45, is_recommended: true },
  { label: "Claude Sonnet 5.5 (Medium)", model_id: "claude-sonnet-5-5-medium", percentage: 90, is_recommended: true },
  { label: "Claude Sonnet 5.5 (High)", model_id: "claude-sonnet-5-5-high", percentage: 90, is_recommended: false },
  { label: "Claude Opus 5.5 (High)", model_id: "claude-opus-5-5-high", percentage: 90, is_recommended: true },
  { label: "GPT-OSS 120B (Medium)", model_id: "gpt-oss-120b-medium", percentage: 90, is_recommended: false },
];

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

function getLocalTimeString() {
  const now = new Date();
  return now.toLocaleTimeString("ja-JP", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

function getFilteredModels(groupKey) {
  const models = demoMode ? DEMO_MODELS : (cachedData?.models || []);
  return models.filter((m) => {
    const text = `${m.label} ${m.model_id}`.toLowerCase();
    if (groupKey === "gemini") {
      return text.includes("gemini") && !text.includes("argon");
    } else if (groupKey === "argon") {
      return text.includes("argon");
    } else if (groupKey === "claude") {
      return text.includes("claude") || text.includes("gpt");
    }
    return false;
  });
}

function renderModelBadges(groupKey) {
  if (!modelBadges || !modelCount) return;
  const filtered = getFilteredModels(groupKey);
  modelBadges.innerHTML = "";

  if (filtered.length === 0) {
    modelCount.textContent = "0";
    const span = document.createElement("span");
    span.className = "model-badge empty";
    span.textContent = "モデル情報なし";
    modelBadges.appendChild(span);
    return;
  }

  modelCount.textContent = `${filtered.length} models`;
  filtered.forEach((m) => {
    const badge = document.createElement("span");
    badge.className = `model-badge ${m.is_recommended ? "recommended" : ""}`;
    badge.textContent = m.label;
    const pct = typeof m.percentage === "number" ? `${m.percentage.toFixed(0)}%` : "--%";
    badge.title = `${m.model_id}\nクォータ残量: ${pct}`;
    modelBadges.appendChild(badge);
  });
}

function findGroupData(groupKey) {
  if (demoMode) {
    if (groupKey === "gemini") {
      return {
        displayName: "Gemini Models",
        modelName: "Gemini Models",
        fiveHour: { fraction: 0.45, pct: 45, seconds: 1 * 3600 + 24 * 60 },
        weekly: { fraction: 0.65, pct: 65, seconds: 3 * 86400 + 12 * 3600 },
      };
    } else if (groupKey === "argon") {
      return {
        displayName: "Gemini 4 Argon",
        modelName: "Gemini 4 Argon",
        fiveHour: null,
        weekly: { fraction: 0.18, pct: 18, seconds: 1 * 86400 + 8 * 3600 },
      };
    } else {
      return {
        displayName: "Claude and GPT models",
        modelName: "Claude & GPT",
        fiveHour: { fraction: 0.90, pct: 90, seconds: 2 * 3600 + 15 * 60 },
        weekly: { fraction: 0.85, pct: 85, seconds: 5 * 86400 + 4 * 3600 },
      };
    }
  }

  if (!cachedData || !cachedData.groups) return null;

  let targetGroup = null;
  for (const g of cachedData.groups) {
    const name = g.display_name.toLowerCase();
    if (groupKey === "gemini" && name.includes("gemini") && !name.includes("argon")) {
      targetGroup = g;
      break;
    } else if (groupKey === "argon" && name.includes("argon")) {
      targetGroup = g;
      break;
    } else if (groupKey === "claude" && (name.includes("claude") || name.includes("gpt"))) {
      targetGroup = g;
      break;
    }
  }

  if (!targetGroup) return null;

  return {
    displayName: targetGroup.display_name,
    modelName:
      groupKey === "gemini"
        ? "Gemini Models"
        : groupKey === "argon"
        ? "Gemini 4 Argon"
        : "Claude & GPT",
    fiveHour: targetGroup.five_hour_bucket
      ? {
          fraction: targetGroup.five_hour_bucket.remaining_fraction,
          pct: targetGroup.five_hour_bucket.percentage,
          seconds: targetGroup.five_hour_bucket.remaining_seconds,
        }
      : null,
    weekly: targetGroup.weekly_bucket
      ? {
          fraction: targetGroup.weekly_bucket.remaining_fraction,
          pct: targetGroup.weekly_bucket.percentage,
          seconds: targetGroup.weekly_bucket.remaining_seconds,
        }
      : null,
  };
}

function renderCurrentGroup() {
  // タブのactive状態の更新
  tabButtons.forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.group === currentGroup);
  });

  // モデルバッジ一覧の描画
  renderModelBadges(currentGroup);

  const groupData = findGroupData(currentGroup);

  if (!groupData) {
    updateGauge(ring5h, val5h, 1.0, 100);
    reset5h.textContent = "リセット: --";
    model5h.textContent = "待機中";
    updateGauge(ringWeekly, valWeekly, 1.0, 100);
    resetWeekly.textContent = "リセット: --";
    modelWeekly.textContent = "待機中";
    return;
  }

  // 5-Hour Limit または 5h枠なし表示
  if (groupData.fiveHour) {
    card5h.style.display = "flex";
    cardNo5h.style.display = "none";
    fiveHourSeconds = groupData.fiveHour.seconds;
    updateGauge(ring5h, val5h, groupData.fiveHour.fraction, groupData.fiveHour.pct);
    reset5h.textContent = `リセット: ${formatDuration(fiveHourSeconds)}`;
    model5h.textContent = groupData.modelName;
  } else {
    // Argonなど5h制限なしの場合
    card5h.style.display = "none";
    cardNo5h.style.display = "flex";
    fiveHourSeconds = 0;
  }

  // Weekly Limit
  if (groupData.weekly) {
    cardWeekly.style.display = "flex";
    weeklySeconds = groupData.weekly.seconds;
    updateGauge(ringWeekly, valWeekly, groupData.weekly.fraction, groupData.weekly.pct);
    resetWeekly.textContent = `リセット: ${formatDuration(weeklySeconds)}`;
    modelWeekly.textContent = groupData.modelName;
  } else {
    updateGauge(ringWeekly, valWeekly, 1.0, 100);
    resetWeekly.textContent = "リセット: 正常";
    modelWeekly.textContent = groupData.modelName;
  }
}

async function fetchStatus() {
  if (demoMode) return;
  btnRefresh.classList.add("spinning");

  try {
    const invoke = window.__TAURI__?.core?.invoke;
    if (!invoke) {
      console.warn("Tauri invoke is not available in browser mode");
      btnRefresh.classList.remove("spinning");
      return;
    }

    const data = await invoke("get_quota_status");
    cachedData = data;

    if (data.connected) {
      statusDot.className = "status-dot connected";
      statusDot.title = `接続中 (${data.email})`;
      planBadge.textContent = data.plan_name;
      creditCount.textContent = (data.prompt_credits || 0).toLocaleString();
      lastUpdated.textContent = getLocalTimeString();

      renderCurrentGroup();
    } else {
      statusDot.className = "status-dot disconnected";
      statusDot.title = data.status_message;
      planBadge.textContent = "待機中";
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

// タブ切り替えクリック
tabButtons.forEach((btn) => {
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    currentGroup = btn.dataset.group;
    renderCurrentGroup();
  });
});

// プランバッジクリックでデモモード切り替え
planBadge.addEventListener("click", (e) => {
  e.stopPropagation();
  demoMode = !demoMode;
  if (demoMode) {
    statusDot.className = "status-dot connected";
    statusDot.title = "デモ表示中 (クリックで戻る)";
    planBadge.textContent = "DEMO";
    creditCount.textContent = "350";
    lastUpdated.textContent = getLocalTimeString();
    renderCurrentGroup();
  } else {
    fetchStatus();
  }
});

// タイトルバー領域のドラッグ移動ハンドラー
const gadgetHeader = document.querySelector(".gadget-header");
if (gadgetHeader) {
  gadgetHeader.addEventListener("mousedown", async (e) => {
    if (
      e.target.closest("button") ||
      e.target.closest(".icon-btn") ||
      e.target.closest(".window-controls") ||
      e.target.closest(".plan-badge")
    ) {
      return;
    }
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

// 初期読み込みと10分ポーリング
window.addEventListener("DOMContentLoaded", () => {
  fetchStatus();
  setInterval(fetchStatus, 10 * 60 * 1000);
});
