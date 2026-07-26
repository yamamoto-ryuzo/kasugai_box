const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

async function loadSavedConfig() {
  const config = await invoke("load_saved_config");
  $("folder-url").value = config.folderUrl || "";
  $("mcp-client-id").value = config.clientId || "";
  $("mcp-client-secret").value = config.clientSecret || "";
  $("mcp-box-subject-type").value = config.boxSubjectType || "enterprise";
  $("mcp-box-subject-id").value = config.boxSubjectId || "";
}

async function saveMcpSettings() {
  const config = {
    clientId: $("mcp-client-id").value.trim(),
    clientSecret: $("mcp-client-secret").value.trim(),
    boxSubjectType: $("mcp-box-subject-type").value.trim(),
    boxSubjectId: $("mcp-box-subject-id").value.trim(),
    folderUrl: $("folder-url")?.value.trim() || "",
  };
  try {
    await invoke("save_config_cmd", { config });
    $("mcp-settings-status").textContent = "保存しました";
    loadSavedConfig();
  } catch (err) {
    $("mcp-settings-status").textContent = `エラー: ${err}`;
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
  const clientId = $("mcp-client-id").value.trim();
  const clientSecret = $("mcp-client-secret").value.trim();
  const boxSubjectType = $("mcp-box-subject-type").value.trim();
  const boxSubjectId = $("mcp-box-subject-id").value.trim();
  const folderUrl = $("folder-url").value.trim();
  const outputDir = $("output-dir").value.trim() || "box_photo_geo_url/output";

  if (!clientId || !clientSecret || !boxSubjectId || !folderUrl) {
    $("status").textContent = "クライアントID、シークレット、Subject ID、フォルダURLを入力してください。";
    return;
  }

  $("run-btn").disabled = true;
  $("status").textContent = "処理中...";
  $("results").hidden = true;

  try {
    await invoke("save_config_cmd", {
      config: { clientId, clientSecret, boxSubjectType, boxSubjectId, folderUrl },
    });
    const result = await invoke("process_photos", {
      clientId,
      clientSecret,
      boxSubjectType,
      boxSubjectId,
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
  const text = input.value.trim();
  if (!text) return;
  addChatMessage("user", text);
  input.value = "";

  try {
    const response = await invoke("mcp_chat", { text });
    addChatMessage("assistant", response.reply);
  } catch (err) {
    addChatMessage("assistant", `エラー: ${err}`);
  }
}

function addChatMessage(role, text) {
  const container = $("chat-messages");
  const row = document.createElement("div");
  row.className = `chat-message ${role}`;
  const bubble = document.createElement("div");
  bubble.className = "chat-bubble";
  bubble.textContent = text;
  row.appendChild(bubble);
  container.appendChild(row);
  container.scrollTop = container.scrollHeight;
}

window.addEventListener("DOMContentLoaded", () => {
  loadSavedConfig();
  initTabs();
  $("run-btn").addEventListener("click", run);
  $("chat-send").addEventListener("click", sendChat);
  $("chat-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") sendChat();
  });
  $("mcp-save-settings")?.addEventListener("click", saveMcpSettings);
});
