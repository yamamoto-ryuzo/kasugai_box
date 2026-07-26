const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);
let hasMore = false;
const MAX_SEARCH_LIMIT = 200;

async function loadSavedConfig() {
  const config = await invoke("load_saved_config");
  $("folder-url").value = config.folderUrl || "";
  $("box-client-id").value = config.clientId || "";
  $("box-client-secret").value = config.clientSecret || "";
  $("box-developer-token").value = config.developerToken || "";
  $("mcp-server-url").value = config.mcpServerUrl || "";
  $("mcp-connection-token").value = config.mcpConnectionToken || "";
  $("box-search-limit").value = config.searchLimit ?? 100;
  updateOauthStatus();
}

async function saveSettings() {
  const config = {
    clientId: $("box-client-id").value.trim(),
    clientSecret: $("box-client-secret").value.trim(),
    developerToken: $("box-developer-token").value.trim() || null,
    folderUrl: $("folder-url")?.value.trim() || "",
    mcpServerUrl: $("mcp-server-url").value.trim() || null,
    mcpConnectionToken: $("mcp-connection-token").value.trim() || null,
    searchLimit: Math.min(MAX_SEARCH_LIMIT, Math.max(1, parseInt($("box-search-limit").value) || 100)),
  };
  try {
    await invoke("save_config_cmd", { config });
    $("settings-status").textContent = "保存しました";
    loadSavedConfig();
  } catch (err) {
    $("settings-status").textContent = `エラー: ${err}`;
  }
}

async function updateOauthStatus() {
  try {
    const status = await invoke("box_oauth_status");
    const text = status.loggedIn
      ? `ログイン中（有効期限: ${new Date(status.expiresAt * 1000).toLocaleString()}）`
      : "未ログイン";
    $("box-oauth-status").textContent = text;
  } catch (err) {
    $("box-oauth-status").textContent = `OAuth 状態取得エラー: ${err}`;
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
    const message = await invoke("developer_token_login", { token });
    $("box-oauth-status").textContent = message;
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err}`;
  }
}

async function loginBoxOAuthAuto() {
  const clientId = $("box-client-id").value.trim();
  const clientSecret = $("box-client-secret").value.trim();
  if (!clientId || !clientSecret) {
    $("box-oauth-status").textContent = "クライアントIDとシークレットを入力してください";
    return;
  }
  $("box-oauth-status").textContent = "ブラウザでログインしてください...";
  try {
    const message = await invoke("box_oauth_login", { clientId, clientSecret });
    $("box-oauth-status").textContent = message;
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err}`;
  }
}

async function logoutBoxOAuth() {
  try {
    await invoke("box_oauth_logout");
    $("box-oauth-status").textContent = "ログアウトしました";
  } catch (err) {
    $("box-oauth-status").textContent = `エラー: ${err}`;
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
      <td>${escapeHtml(r.full_name)}</td>
      <td>${r.latitude ?? ""}</td>
      <td>${r.longitude ?? ""}</td>
      <td>${r.date_taken ?? ""}</td>
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

async function run() {
  const clientId = $("box-client-id").value.trim();
  const clientSecret = $("box-client-secret").value.trim();
  const folderUrl = $("folder-url").value.trim();
  const outputDir = $("output-dir").value.trim() || "box_photo_geo_url/output";

  if (!clientId || !clientSecret || !folderUrl) {
    $("status").textContent = "クライアントID、シークレット、フォルダURLを入力してください。";
    return;
  }

  $("run-btn").disabled = true;
  $("status").textContent = "処理中...";
  $("results").hidden = true;

  try {
    await invoke("save_config_cmd", {
      config: { clientId, clientSecret, folderUrl },
    });
    const result = await invoke("process_photos", {
      clientId,
      clientSecret,
      folderUrl,
      outputDir,
    });
    $("status").innerHTML = `
      <p>${result.message}</p>
      <p>CSV: ${result.csvPath}</p>
      ${result.geojsonPath ? `<p>GeoJSON: ${result.geojsonPath}</p>` : ""}
    `;
    renderRecords(result.records);
  } catch (err) {
    $("status").textContent = `エラー: ${err}`;
  } finally {
    $("run-btn").disabled = false;
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
    const response = await invoke("box_api_chat", { text, searchLimit });
    hasMore = response.hasMore;
    addChatMessage("assistant", response.reply, true);
  } catch (err) {
    hasMore = false;
    addChatMessage("assistant", `エラー: ${err}`);
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
      invoke("copy_to_clipboard", { text: copyText });
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
    const text = await invoke("mcp_list_tools");
    result.textContent = text;
    status.textContent = "完了";
  } catch (err) {
    status.textContent = `エラー: ${err}`;
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
    const text = await invoke("mcp_call_tool", { name, arguments: args });
    result.textContent = text;
    status.textContent = "完了";
  } catch (err) {
    status.textContent = `エラー: ${err}`;
  }
}

window.addEventListener("DOMContentLoaded", () => {
  loadSavedConfig();
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

  $("chat-messages").addEventListener("click", (e) => {
    const a = e.target.closest("a[target='_blank']");
    if (a) {
      e.preventDefault();
      invoke("open_url", { url: a.href });
    }
  });
});
