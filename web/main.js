const $ = (id) => document.getElementById(id);
let hasMore = false;
const MAX_SEARCH_LIMIT = 200;
const SAVED_PLACEHOLDER = "保存済み（変更する場合のみ入力）";

async function api(path, options = {}) {
  const resp = await fetch(path, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  const text = await resp.text();
  let data = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = null;
  }
  if (!resp.ok) {
    throw new Error(data?.error || `HTTP ${resp.status}`);
  }
  return data;
}

const apiGet = (path) => api(path);
const apiPost = (path, body) => api(path, { method: "POST", body: JSON.stringify(body ?? {}) });

async function loadSavedConfig() {
  try {
    const config = await apiGet("/api/v1/config");
    $("folder-url").value = config.folderUrl || "";
    $("box-client-id").placeholder = config.hasClientId ? SAVED_PLACEHOLDER : "Box Client ID";
    $("box-client-secret").placeholder = config.hasClientSecret ? SAVED_PLACEHOLDER : "Box Client Secret";
    $("box-developer-token").placeholder = config.hasDeveloperToken ? SAVED_PLACEHOLDER : "Box Developer Token";
    $("mcp-server-url").value = config.mcpServerUrl || "";
    $("mcp-connection-token").placeholder = config.hasMcpConnectionToken ? SAVED_PLACEHOLDER : "MCP connection token";
    $("box-search-limit").value = config.searchLimit ?? 100;
    $("server-port-input").value = config.port ?? 8410;
  } catch (err) {
    $("settings-status").textContent = `設定取得エラー: ${err.message}`;
  }
  loadServerStatus();
  updateOauthStatus();
}

async function loadServerStatus() {
  try {
    const status = await apiGet("/api/v1/server/status");
    $("server-port").textContent = status.port ?? "-";
    $("server-status-text").textContent = "実行中";
  } catch (err) {
    $("server-port").textContent = "-";
    $("server-status-text").textContent = `状態取得エラー: ${err.message}`;
  }
}

async function stopServer() {
  if (!confirm("kasugai_box を停止しますか？KASUGAI 本体からの再起動が必要になります。")) {
    return;
  }
  $("server-status-text").textContent = "停止しています...";
  try {
    await apiPost("/api/v1/server/stop");
    $("server-status-text").textContent = "停止しました。このタブを閉じてください。";
    $("server-stop").disabled = true;
  } catch (err) {
    $("server-status-text").textContent = `停止エラー: ${err.message}`;
  }
}

async function saveSettings() {
  const config = {
    clientId: $("box-client-id").value.trim() || null,
    clientSecret: $("box-client-secret").value.trim() || null,
    developerToken: $("box-developer-token").value.trim() || null,
    folderUrl: $("folder-url")?.value.trim() || "",
    mcpServerUrl: $("mcp-server-url").value.trim(),
    mcpConnectionToken: $("mcp-connection-token").value.trim() || null,
    searchLimit: Math.min(MAX_SEARCH_LIMIT, Math.max(1, parseInt($("box-search-limit").value) || 100)),
    port: Math.min(65535, Math.max(1, parseInt($("server-port-input").value) || 8410)),
  };
  try {
    await apiPost("/api/v1/config", config);
    $("settings-status").textContent = "保存しました";
    $("box-client-id").value = "";
    $("box-client-secret").value = "";
    $("box-developer-token").value = "";
    $("mcp-connection-token").value = "";
    loadSavedConfig();
  } catch (err) {
    $("settings-status").textContent = `エラー: ${err.message}`;
  }
}

async function updateOauthStatus() {
  try {
    const status = await apiGet("/api/v1/auth/box/status");
    const text = status.loggedIn
      ? `ログイン中（有効期限: ${status.expiresAt ? new Date(status.expiresAt * 1000).toLocaleString() : "デベロッパートークン"}）`
      : "未ログイン";
    $("box-oauth-status").textContent = text;
  } catch (err) {
    $("box-oauth-status").textContent = `OAuth 状態取得エラー: ${err.message}`;
  }
}

async function developerTokenLogin() {
  const token = $("box-developer-token").value.trim();
  if (!token) {
    $("box-oauth-status").textContent = "設定タブでデベロッパートークンを入力してください";
    return;
  }
  $("box-oauth-status").textContent = "ログイン確認中...";
  try {
    const result = await apiPost("/api/v1/auth/box/developer-token", { token });
    $("box-oauth-status").textContent = result.message;
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err.message}`;
  }
}

async function loginBoxOAuthAuto() {
  $("box-oauth-status").textContent = "ブラウザでログインしてください...";
  try {
    const body = {
      clientId: $("box-client-id").value.trim() || null,
      clientSecret: $("box-client-secret").value.trim() || null,
    };
    const result = await apiPost("/api/v1/auth/box/login", body);
    $("box-oauth-status").textContent = result.message;
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err.message}`;
  }
}

async function logoutBoxOAuth() {
  try {
    await apiPost("/api/v1/auth/box/logout");
    $("box-oauth-status").textContent = "ログアウトしました";
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err.message}`;
  }
}

function renderRecords(records) {
  const tbody = $("results").querySelector("tbody");
  tbody.innerHTML = "";
  if (!records || records.length === 0) {
    $("results").hidden = true;
    return;
  }
  for (const r of records) {
    const row = document.createElement("tr");
    const link = document.createElement("a");
    link.href = r.url;
    link.textContent = r.url;
    link.target = "_blank";
    row.innerHTML = `
      <td>${escapeHtml(r.fullName ?? "")}</td>
      <td>${r.latitude ?? ""}</td>
      <td>${r.longitude ?? ""}</td>
      <td>${r.dateTaken ?? ""}</td>
      <td></td>
    `;
    row.lastElementChild.appendChild(link);
    tbody.appendChild(row);
  }
  $("results").hidden = false;
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function pollJob(jobId) {
  for (;;) {
    const job = await apiGet(`/api/v1/jobs/${jobId}`);
    if (job.status === "succeeded") return job.result;
    if (job.status === "failed") throw new Error(job.error || "ジョブが失敗しました");
    $("status").textContent = `処理中... ${job.progress ?? 0}%`;
    await sleep(1000);
  }
}

async function run() {
  const folderUrl = $("folder-url").value.trim();
  const outputDir = $("output-dir").value.trim() || "c:/kasugai/box/photo";

  if (!folderUrl) {
    $("status").textContent = "フォルダURLを入力してください。";
    return;
  }

  $("run-btn").disabled = true;
  $("status").textContent = "処理中...";
  $("results").hidden = true;

  try {
    const accepted = await apiPost("/api/v1/photos/process", { folderUrl, outputDir });
    const result = await pollJob(accepted.jobId);
    $("status").innerHTML = `
      <p>${escapeHtml(result.message)}</p>
      <p>CSV: ${escapeHtml(result.csvPath)}</p>
      ${result.geojsonPath ? `<p>GeoJSON: ${escapeHtml(result.geojsonPath)}</p>` : ""}
    `;
    renderRecords(result.records);
  } catch (err) {
    $("status").textContent = `エラー: ${err.message}`;
  } finally {
    $("run-btn").disabled = false;
  }
}

function compareVersion(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const na = pa[i] || 0;
    const nb = pb[i] || 0;
    if (na > nb) return 1;
    if (na < nb) return -1;
  }
  return 0;
}

async function loadVersion() {
  try {
    const health = await apiGet("/health");
    $("current-version").textContent = health.version ?? "-";
    await checkUpdate(health.version);
  } catch (err) {
    $("current-version").textContent = "-";
    $("version-status").textContent = `バージョン取得エラー: ${err.message}`;
  }
}

async function checkUpdate(currentVersion) {
  $("version-status").textContent = "最新情報を確認中...";
  $("download-update").hidden = true;
  try {
    const latest = await apiGet("/api/v1/update/latest");
    const latestVersion = latest.version ?? "-";
    const url = latest.platforms?.["windows-x86_64"]?.url;
    $("latest-version").textContent = latestVersion;
    if (!currentVersion || currentVersion === "-") {
      $("update-status").textContent = "";
      $("version-status").textContent = "現在のバージョンが取得できません";
      return;
    }
    const cmp = compareVersion(currentVersion, latestVersion);
    if (cmp < 0) {
      $("update-status").textContent = "（新しいバージョンがあります）";
      $("version-status").textContent = "更新が利用可能です";
      if (url) {
        $("download-update").href = url;
        $("download-update").hidden = false;
      }
    } else if (cmp === 0) {
      $("update-status").textContent = "（最新です）";
      $("version-status").textContent = "";
    } else {
      $("update-status").textContent = "（現在のバージョンの方が新しいです）";
      $("version-status").textContent = "";
    }
  } catch (err) {
    $("latest-version").textContent = "-";
    $("version-status").textContent = `更新確認エラー: ${err.message}`;
  }
}

function initTabs() {
  const buttons = document.querySelectorAll(".tab-btn");
  const panels = document.querySelectorAll(".tab-panel");

  for (const btn of buttons) {
    btn.addEventListener("click", () => {
      const target = btn.dataset.tab;
      for (const b of buttons) b.classList.remove("active");
      for (const p of panels) p.classList.remove("active");
      btn.classList.add("active");
      $(`tab-${target}`).classList.add("active");
    });
  }
}

async function sendChat() {
  const input = $("chat-input");
  let text = input.value.trim();
  if (!text) {
    if (hasMore) {
      text = "more";
    } else {
      return;
    }
  }
  addChatMessage("user", text);
  input.value = "";

  try {
    const searchLimit = Math.min(MAX_SEARCH_LIMIT, Math.max(1, parseInt($("box-search-limit").value) || 100));
    const response = await apiPost("/api/v1/box/chat", { text, searchLimit });
    hasMore = response.hasMore;
    addChatMessage("assistant", response.reply, true);
  } catch (err) {
    hasMore = false;
    addChatMessage("assistant", `エラー: ${err.message}`);
  }
}

function addChatMessage(role, text, isHtml = false) {
  const container = $("chat-messages");
  const row = document.createElement("div");
  row.className = `chat-message ${role}`;
  const bubble = document.createElement("div");
  bubble.className = "chat-bubble";
  if (isHtml) {
    bubble.innerHTML = text;
  } else {
    bubble.textContent = text;
  }
  row.appendChild(bubble);

  if (role === "assistant") {
    const copyBtn = document.createElement("button");
    copyBtn.type = "button";
    copyBtn.className = "copy-all";
    copyBtn.textContent = "コピー";
    copyBtn.addEventListener("click", () => {
      const links = bubble.querySelectorAll("a[target='_blank']");
      let copyText;
      if (links.length > 0) {
        copyText = Array.from(links).map((a) => `[${a.textContent}](${a.href})`).join("\n");
      } else {
        copyText = bubble.textContent;
      }
      navigator.clipboard.writeText(copyText);
    });
    row.appendChild(copyBtn);
  }

  container.appendChild(row);
  container.scrollTop = container.scrollHeight;
}

async function listMcpTools() {
  const status = $("mcp-status");
  const result = $("mcp-result");
  status.textContent = "取得中...";
  try {
    const data = await apiGet("/api/v1/mcp-client/tools");
    result.textContent = data.result;
    status.textContent = "完了";
  } catch (err) {
    status.textContent = `エラー: ${err.message}`;
  }
}

async function callMcpTool() {
  const name = $("mcp-tool-name").value.trim();
  const args = $("mcp-tool-args").value.trim();
  const status = $("mcp-status");
  const result = $("mcp-result");
  if (!name) {
    status.textContent = "ツール名を入力してください";
    return;
  }
  status.textContent = "実行中...";
  try {
    const data = await apiPost("/api/v1/mcp-client/call", { name, arguments: args });
    result.textContent = data.result;
    status.textContent = "完了";
  } catch (err) {
    status.textContent = `エラー: ${err.message}`;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  loadSavedConfig();
  loadVersion();
  initTabs();
  $("run-btn").addEventListener("click", run);
  $("chat-send").addEventListener("click", sendChat);
  $("chat-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") sendChat();
  });
  $("save-settings")?.addEventListener("click", saveSettings);
  $("box-search-limit")?.addEventListener("change", saveSettings);
  $("box-developer-token-login")?.addEventListener("click", developerTokenLogin);
  $("box-oauth-auto")?.addEventListener("click", loginBoxOAuthAuto);
  $("box-oauth-logout")?.addEventListener("click", logoutBoxOAuth);
  $("mcp-list-tools")?.addEventListener("click", listMcpTools);
  $("mcp-call-tool")?.addEventListener("click", callMcpTool);
  $("server-stop")?.addEventListener("click", stopServer);
  $("check-update")?.addEventListener("click", () => {
    const current = $("current-version").textContent;
    checkUpdate(current);
  });
});
