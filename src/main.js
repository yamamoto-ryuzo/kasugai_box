const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

async function loadSavedConfig() {
  const config = await invoke("load_saved_config");
  $("token").value = config.accessToken || "";
  $("folder-url").value = config.folderUrl || "";
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
  const token = $("token").value.trim();
  const folderUrl = $("folder-url").value.trim();
  const outputDir = $("output-dir").value.trim() || "box_photo_geo_url/output";

  if (!token || !folderUrl) {
    $("status").textContent = "トークンとフォルダURLを入力してください。";
    return;
  }

  $("run-btn").disabled = true;
  $("status").textContent = "処理中...";
  $("results").hidden = true;

  try {
    await invoke("save_config_cmd", {
      config: { accessToken: token, folderUrl },
    });
    const result = await invoke("process_photos", {
      token,
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

window.addEventListener("DOMContentLoaded", () => {
  loadSavedConfig();
  $("run-btn").addEventListener("click", run);
});
