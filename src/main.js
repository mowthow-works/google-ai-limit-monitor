const CIRCUMFERENCE = 2 * Math.PI * 35; // r=35 -> ~219.911

// DOM要素
const statusDot = document.getElementById("status-dot");
const planBadge = document.getElementById("plan-badge");
const lastUpdated = document.getElementById("last-updated");
const creditBadge = document.getElementById("credit-badge");
const creditCount = document.getElementById("credit-count");
const creditUnit = document.getElementById("credit-unit");
const sourceBadge = document.getElementById("source-badge");

const card5h = document.getElementById("card-5h");
const cardNo5h = document.getElementById("card-no-5h");
const no5hDesc = document.getElementById("no-5h-desc");
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

const groupTabs = document.getElementById("group-tabs");

let currentGroupId = null;
let cachedData = null;
let fiveHourSeconds = 0;
let weeklySeconds = 0;
let demoMode = false;

const DEMO_GROUPS = [
  {
    id: "gemini",
    display_name: "Gemini Models",
    tab_label: "Gemini",
    description: "Models within this group: Gemini 4 Neon, Gemini 4 Argon, Gemini 3.8 Flash",
    five_hour_bucket: {
      remaining_fraction: 0.45,
      percentage: 45,
      remaining_seconds: 1 * 3600 + 24 * 60,
    },
    weekly_bucket: {
      remaining_fraction: 0.65,
      percentage: 65,
      remaining_seconds: 3 * 86400 + 12 * 3600,
    },
  },
  {
    id: "claude",
    display_name: "Claude and GPT models",
    tab_label: "Claude / GPT",
    description: "Models within this group: Claude Opus 5.5, Claude Sonnet 5.5, GPT-OSS 120B",
    five_hour_bucket: {
      remaining_fraction: 0.90,
      percentage: 90,
      remaining_seconds: 2 * 3600 + 15 * 60,
    },
    weekly_bucket: {
      remaining_fraction: 0.85,
      percentage: 85,
      remaining_seconds: 5 * 86400 + 4 * 3600,
    },
  },
];

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

function getActiveGroups() {
  if (demoMode) return DEMO_GROUPS;
  if (!cachedData || !Array.isArray(cachedData.groups)) return [];
  return cachedData.groups.filter((g) => Boolean(g.five_hour_bucket || g.weekly_bucket));
}

function resolveActiveGroupId(groups, currentId) {
  if (!Array.isArray(groups) || groups.length === 0) return null;
  if (currentId && groups.some((g) => g.id === currentId)) {
    return currentId;
  }
  return groups[0].id;
}

function renderTabs(groups) {
  if (!groupTabs) return;
  groupTabs.innerHTML = "";

  if (!groups || groups.length === 0) {
    groupTabs.style.display = "none";
    return;
  }

  groupTabs.style.display = "flex";
  groups.forEach((g) => {
    const btn = document.createElement("button");
    btn.className = `tab-btn ${g.id === currentGroupId ? "active" : ""}`;
    btn.dataset.group = g.id;
    btn.textContent = g.tab_label || g.display_name || g.id;
    btn.title = g.display_name || g.tab_label || g.id;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      currentGroupId = g.id;
      renderCurrentGroup();
    });
    groupTabs.appendChild(btn);
  });
}

function getFilteredModels(targetGroup, activeGroups) {
  if (!targetGroup) return [];
  const models = demoMode ? DEMO_MODELS : (cachedData?.models || []);
  const hasArgonTab = activeGroups.some(
    (g) => g.id === "argon" || (g.display_name || "").toLowerCase().includes("argon")
  );
  const gId = (targetGroup.id || "").toLowerCase();
  const gName = (targetGroup.display_name || "").toLowerCase();
  const gDesc = (targetGroup.description || "").toLowerCase();

  return models.filter((m) => {
    const text = `${m.label} ${m.model_id}`.toLowerCase();
    if (gId === "argon" || gName.includes("argon")) {
      return text.includes("argon");
    }
    if (gId === "gemini" || gName.includes("gemini")) {
      if (hasArgonTab) {
        return text.includes("gemini") && !text.includes("argon");
      }
      // Argon 専用枠が Gemini に統合された構成では Argon モデルも Gemini グループへ統合表示する
      return text.includes("gemini") || text.includes("argon");
    }
    if (gId === "claude" || gName.includes("claude") || gName.includes("gpt")) {
      return text.includes("claude") || text.includes("gpt");
    }
    const baseLabel = (m.label || "").split("(")[0].trim().toLowerCase();
    return Boolean(baseLabel && (gDesc.includes(baseLabel) || gName.includes(baseLabel)));
  });
}

function isAgyMode() {
  return !demoMode && Boolean(cachedData?.connected && cachedData?.source === "agy");
}

function renderModelBadges(targetGroup, activeGroups) {
  if (!modelBadges || !modelCount) return;

  if (isAgyMode()) {
    modelCount.textContent = "―";
    modelBadges.innerHTML = "";
    const span = document.createElement("span");
    span.className = "model-badge empty";
    span.textContent = "―（agy 取得時はモデル詳細なし）";
    modelBadges.appendChild(span);
    return;
  }

  const filtered = getFilteredModels(targetGroup, activeGroups);
  modelBadges.innerHTML = "";

  if (filtered.length === 0) {
    modelCount.textContent = "0";
    const span = document.createElement("span");
    span.className = "model-badge empty";
    span.textContent = targetGroup ? "モデル情報なし" : "待機中...";
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

function renderCurrentGroup() {
  const activeGroups = getActiveGroups();
  currentGroupId = resolveActiveGroupId(activeGroups, currentGroupId);

  // タブバーの動的再描画
  renderTabs(activeGroups);

  const targetGroup = activeGroups.find((g) => g.id === currentGroupId) || null;

  // モデルバッジ一覧の描画
  renderModelBadges(targetGroup, activeGroups);

  if (!targetGroup) {
    fiveHourSeconds = 0;
    weeklySeconds = 0;
    card5h.style.display = "flex";
    cardNo5h.style.display = "none";
    updateGauge(ring5h, val5h, 1.0, 100);
    val5h.textContent = "--%";
    val5h.style.color = "#fff";
    reset5h.textContent = "リセット: --";
    model5h.textContent = "待機中";
    updateGauge(ringWeekly, valWeekly, 1.0, 100);
    valWeekly.textContent = "--%";
    valWeekly.style.color = "#fff";
    resetWeekly.textContent = "リセット: --";
    modelWeekly.textContent = "待機中";
    return;
  }

  const groupTitle = targetGroup.display_name || targetGroup.tab_label || targetGroup.id;

  // 5-Hour Limit または 5h枠なし表示
  if (targetGroup.five_hour_bucket) {
    card5h.style.display = "flex";
    cardNo5h.style.display = "none";
    fiveHourSeconds = targetGroup.five_hour_bucket.remaining_seconds;
    updateGauge(
      ring5h,
      val5h,
      targetGroup.five_hour_bucket.remaining_fraction,
      targetGroup.five_hour_bucket.percentage
    );
    reset5h.textContent = `リセット: ${formatDuration(fiveHourSeconds)}`;
    model5h.textContent = groupTitle;
  } else {
    // 5h制限なし（Weekly専用グループ）の場合
    card5h.style.display = "none";
    cardNo5h.style.display = "flex";
    if (no5hDesc) {
      no5hDesc.textContent = `${targetGroup.tab_label || groupTitle}はWeekly枠のみで運用されます`;
    }
    fiveHourSeconds = 0;
  }

  // Weekly Limit
  if (targetGroup.weekly_bucket) {
    cardWeekly.style.display = "flex";
    weeklySeconds = targetGroup.weekly_bucket.remaining_seconds;
    updateGauge(
      ringWeekly,
      valWeekly,
      targetGroup.weekly_bucket.remaining_fraction,
      targetGroup.weekly_bucket.percentage
    );
    resetWeekly.textContent = `リセット: ${formatDuration(weeklySeconds)}`;
    modelWeekly.textContent = groupTitle;
  } else {
    weeklySeconds = 0;
    updateGauge(ringWeekly, valWeekly, 1.0, 100);
    valWeekly.textContent = "--%";
    valWeekly.style.color = "#fff";
    resetWeekly.textContent = "リセット: なし";
    modelWeekly.textContent = groupTitle;
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
      const agyMode = data.source === "agy";
      statusDot.className = "status-dot connected";
      statusDot.title = agyMode
        ? "接続中 (agy フォールバック)"
        : data.email
        ? `接続中 (${data.email})`
        : "接続中 (Language Server)";
      planBadge.textContent = agyMode ? "―" : data.plan_name || "Pro";
      if (sourceBadge) {
        sourceBadge.style.display = agyMode ? "inline-flex" : "none";
      }

      if (agyMode || data.prompt_credits === null || data.prompt_credits === undefined) {
        creditCount.textContent = "―";
        if (creditUnit) creditUnit.textContent = "";
        if (creditBadge) {
          creditBadge.classList.add("muted");
          creditBadge.title = "agy フォールバック時はクレジット残高を取得できません";
        }
      } else {
        creditCount.textContent = data.prompt_credits.toLocaleString();
        if (creditUnit) creditUnit.textContent = " credits";
        if (creditBadge) {
          creditBadge.classList.remove("muted");
          creditBadge.title = "利用可能なPrompt Credits";
        }
      }
      lastUpdated.textContent = getLocalTimeString();

      renderCurrentGroup();
    } else {
      statusDot.className = "status-dot disconnected";
      statusDot.title = data.status_message;
      planBadge.textContent = "待機中";
      if (sourceBadge) {
        sourceBadge.style.display = "none";
      }
      creditCount.textContent = "―";
      if (creditUnit) creditUnit.textContent = "";
      if (creditBadge) {
        creditBadge.classList.add("muted");
        creditBadge.title = "未接続";
      }
      lastUpdated.textContent = getLocalTimeString();

      renderCurrentGroup();
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

// プランバッジクリックでデモモード切り替え
planBadge.addEventListener("click", (e) => {
  e.stopPropagation();
  demoMode = !demoMode;
  if (demoMode) {
    statusDot.className = "status-dot connected";
    statusDot.title = "デモ表示中 (クリックで戻る)";
    planBadge.textContent = "DEMO";
    if (sourceBadge) sourceBadge.style.display = "none";
    if (creditBadge) {
      creditBadge.classList.remove("muted");
      creditBadge.title = "利用可能なPrompt Credits";
    }
    creditCount.textContent = "350";
    if (creditUnit) creditUnit.textContent = " credits";
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

  // トレイメニュー等からの即時更新イベントを受信
  if (window.__TAURI__?.event?.listen) {
    window.__TAURI__.event.listen("trigger-refresh", () => {
      fetchStatus();
    });
  }
});
