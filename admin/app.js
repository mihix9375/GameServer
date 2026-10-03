import { hasUnversionedChanges } from "./version-warning.mjs";
import { bindFileDropzone, uploadFileBatch } from "./upload-files.mjs";

const state = {
  games: [],
  comments: [],
  clients: [],
  status: null,
  token: sessionStorage.getItem("admin-token") || "",
  editorGameId: null,
  rankingLoadedFor: null,
  replacementFile: null,
};
const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];

function headers() {
  return state.token ? { Authorization: `Bearer ${state.token}` } : {};
}

async function api(path, options = {}) {
  const response = await fetch(path, { ...options, headers: { ...headers(), ...(options.headers || {}) } });
  if (response.status === 401) {
    $("#login").classList.remove("hidden");
    const message = state.token ? "管理トークンが違います" : "管理トークンが必要です";
    $("#login-error").textContent = message;
    throw new Error(message);
  }
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.message || `HTTP ${response.status}`);
  return data;
}

function log(message) {
  const row = document.createElement("p");
  const time = document.createElement("time");
  time.textContent = new Date().toLocaleTimeString("ja-JP");
  row.append(time, document.createTextNode(message));
  $("#activity-log").prepend(row);
}

let toastTimer;
function toast(message, error = false) {
  const element = $("#toast");
  element.textContent = message;
  element.className = `toast show${error ? " error" : ""}`;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { element.className = "toast"; }, 3200);
}

function formatDuration(seconds) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return days ? `${days}日 ${hours}時間` : hours ? `${hours}時間 ${minutes}分` : `${minutes}分`;
}

function formatDate(seconds) {
  const date = new Date(Number(seconds) * 1000);
  return Number.isNaN(date.getTime()) ? "--" : date.toLocaleString("ja-JP");
}

function formatBytes(bytes) {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = Number(bytes) || 0;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit += 1; }
  const digits = value >= 100 || unit === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

function formatRate(bytes) {
  return `${formatBytes(bytes)}/s`;
}

function createClientStat(label, value) {
  const stat = document.createElement("div"); stat.className = "client-stat";
  const caption = document.createElement("span"); caption.textContent = label;
  const content = document.createElement("strong"); content.textContent = value;
  stat.append(caption, content);
  return stat;
}

function chartPoints(history, field, maximum) {
  if (!history.length) return "";
  return history.map((point, index) => {
    const x = history.length === 1 ? 0 : index * 600 / (history.length - 1);
    const y = 142 - ((Number(point[field]) || 0) / maximum) * 134;
    return `${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(" ");
}

function createTrafficChart(client) {
  const namespace = "http://www.w3.org/2000/svg";
  const wrapper = document.createElement("div"); wrapper.className = "traffic-chart-wrap";
  const svg = document.createElementNS(namespace, "svg");
  svg.classList.add("traffic-chart"); svg.setAttribute("viewBox", "0 0 600 150"); svg.setAttribute("preserveAspectRatio", "none");
  const maximum = Math.max(1, ...client.history.flatMap((point) => [point.sent_bytes, point.received_bytes]));
  for (const y of [8, 41.5, 75, 108.5, 142]) {
    const line = document.createElementNS(namespace, "line");
    line.classList.add("grid"); line.setAttribute("x1", "0"); line.setAttribute("x2", "600"); line.setAttribute("y1", String(y)); line.setAttribute("y2", String(y));
    svg.append(line);
  }
  const sent = document.createElementNS(namespace, "polyline"); sent.classList.add("sent-line"); sent.setAttribute("points", chartPoints(client.history, "sent_bytes", maximum));
  const received = document.createElementNS(namespace, "polyline"); received.classList.add("received-line"); received.setAttribute("points", chartPoints(client.history, "received_bytes", maximum));
  svg.append(sent, received);
  const scale = document.createElement("div"); scale.className = "traffic-chart-scale";
  for (const ratio of [1, .75, .5, .25, 0]) {
    const label = document.createElement("span"); label.textContent = formatRate(maximum * ratio); scale.append(label);
  }
  wrapper.append(svg, scale);
  return wrapper;
}

function renderClients() {
  const clients = state.clients;
  $("#connected-client-count").textContent = clients.length;
  $("#aggregate-send-rate").textContent = formatRate(clients.reduce((sum, client) => sum + client.sent_bytes_per_second, 0));
  $("#aggregate-receive-rate").textContent = formatRate(clients.reduce((sum, client) => sum + client.received_bytes_per_second, 0));
  const list = $("#clients-list"); list.replaceChildren();
  if (!clients.length) {
    const empty = document.createElement("p"); empty.className = "empty"; empty.textContent = "接続中のLauncherはありません"; list.append(empty); return;
  }
  for (const client of clients) {
    const card = document.createElement("article"); card.className = "client-card";
    const head = document.createElement("div"); head.className = "client-card-head";
    const identity = document.createElement("div"); identity.className = "client-identity";
    const dot = document.createElement("i"); dot.className = "client-online";
    const ip = document.createElement("h3"); ip.textContent = client.ip;
    const time = document.createElement("span"); time.className = "client-time";
    time.textContent = `接続 ${formatDate(client.connected_since)}${client.connections > 1 ? `・${client.connections}接続` : ""}`;
    identity.append(dot, ip); head.append(identity, time);
    const stats = document.createElement("div"); stats.className = "client-stats";
    stats.append(
      createClientStat("送信速度", formatRate(client.sent_bytes_per_second)),
      createClientStat("受信速度", formatRate(client.received_bytes_per_second)),
      createClientStat("累計送信", formatBytes(client.total_sent_bytes)),
      createClientStat("累計受信", formatBytes(client.total_received_bytes)),
    );
    card.append(head, stats, createTrafficChart(client)); list.append(card);
  }
}

async function loadClients() {
  try {
    state.clients = await api("/api/clients");
    renderClients();
  } catch (error) {
    if (!$("#login").classList.contains("hidden")) return;
    console.warn("クライアント情報を取得できません:", error);
  }
}

function gameId(game) {
  return (game.id || game.game || "").replace(/\.exe$/i, "");
}

function renderStatus() {
  const data = state.status;
  $("#sidebar-status").textContent = "Server Online";
  $("#uptime").textContent = `稼働時間 ${formatDuration(data.uptime_seconds)}`;
  $("#access-mode").textContent = data.remote_access ? "REMOTE" : "LOCAL";
  $("#admin-address").textContent = data.admin;
  $("#grpc-v4").textContent = data.grpc[0];
  $("#grpc-v6").textContent = data.grpc[1];
  $("#leaderboard-api").textContent = `http://${data.leaderboard_api}`;
  $("#games-directory").textContent = data.games_directory;
  $("#comments-file").textContent = data.comments_file;
  $("#leaderboards-file").textContent = data.leaderboards_file;
  $("#config-file").textContent = data.config_file;
}

function renderGames() {
  const query = $("#game-search").value.trim().toLowerCase();
  const games = state.games.filter((game) => `${gameId(game)} ${game.title || ""} ${game.author || ""}`.toLowerCase().includes(query));
  const list = $("#games-list");
  list.replaceChildren();
  if (!games.length) {
    const empty = document.createElement("p"); empty.className = "empty"; empty.textContent = "配布ゲームがありません"; list.append(empty); return;
  }
  for (const game of games) {
    const row = document.createElement("article"); row.className = "game-row";
    const head = document.createElement("div"); head.className = "game-row-head";
    const title = document.createElement("h3"); title.textContent = game.title || gameId(game);
    const version = document.createElement("span"); version.className = "version"; version.textContent = `v${game.version || "--"}`;
    const id = document.createElement("p"); id.className = "game-id"; id.textContent = gameId(game);
    const actions = document.createElement("div"); actions.className = "game-actions";
    const edit = document.createElement("button"); edit.className = "button primary"; edit.textContent = "管理・編集";
    edit.addEventListener("click", () => openGameManager(game));
    const notify = document.createElement("button"); notify.className = "button secondary"; notify.textContent = "更新通知を送る";
    notify.addEventListener("click", () => sendNotification(gameId(game), notify));
    const remove = document.createElement("button"); remove.className = "button danger"; remove.textContent = "削除";
    remove.addEventListener("click", () => deleteGame(game, remove));
    head.append(title, version); actions.append(edit, notify, remove); row.append(head, id, actions); list.append(row);
  }
}

function createCommentRow(item) {
  const row = document.createElement("article"); row.className = "comment-row";
  const game = document.createElement("strong"); game.textContent = item.game_id;
  const author = document.createElement("span"); author.textContent = item.author || "匿名";
  const content = document.createElement("span"); content.className = "content"; content.textContent = item.content;
  const date = document.createElement("span"); date.className = "muted"; date.textContent = formatDate(item.created_at);
  const remove = document.createElement("button"); remove.className = "button danger"; remove.textContent = "削除";
  remove.addEventListener("click", () => deleteComment(item, remove));
  row.append(game, author, content, date, remove);
  return row;
}

function renderGameComments() {
  const list = $("#game-comments-list");
  const query = $("#game-comment-search").value.trim().toLowerCase();
  const comments = state.comments.filter((item) => item.game_id === state.editorGameId)
    .filter((item) => `${item.author} ${item.content}`.toLowerCase().includes(query));
  $("#game-comment-count").textContent = `${comments.length}件`;
  list.replaceChildren();
  if (!comments.length) {
    const empty = document.createElement("p"); empty.className = "empty compact"; empty.textContent = "コメントがありません"; list.append(empty); return;
  }
  for (const item of comments) list.append(createCommentRow(item));
}

async function loadAll() {
  try {
    const [status, games, comments] = await Promise.all([api("/api/status"), api("/api/games"), api("/api/comments")]);
    state.status = status; state.games = games; state.comments = comments;
    renderStatus(); renderGames();
    if (state.editorGameId) {
      const editorGame = selectedEditorGame();
      if (editorGame) renderGameComments(); else closeGameManager();
    }
    $("#game-count").textContent = games.length;
    $("#comment-count").textContent = comments.length;
    $("#login").classList.add("hidden");
    log("管理データを更新しました");
  } catch (error) {
    if (!$("#login").classList.contains("hidden")) return;
    toast(error.message, true); log(`エラー: ${error.message}`);
  }
}

async function sendNotification(id, button) {
  button.disabled = true;
  try { const result = await api(`/api/notify/${encodeURIComponent(id)}`, { method: "POST" }); toast(result.message); log(result.message); }
  catch (error) { toast(error.message, true); log(`通知失敗: ${error.message}`); }
  finally { button.disabled = false; }
}

function fillGameDetails(game) {
  $("#edit-id").value = gameId(game);
  $("#edit-title").value = game.title || "";
  $("#edit-version").value = game.version || "";
  $("#edit-author").value = game.author || "";
  $("#edit-date").value = game.latestUpdate || game.latest_update || "";
  $("#edit-tags").value = Array.isArray(game.tags) ? game.tags.join(", ") : "";
  $("#edit-description").value = game.descriptionSource || game.description || "";
  originalGameDetails = readGameDetails();
  updateVersionWarning();
}

let originalGameDetails = null;

function readGameDetails() {
  return {
    title: $("#edit-title").value,
    version: $("#edit-version").value,
    author: $("#edit-author").value,
    latest_update: $("#edit-date").value,
    tags: $("#edit-tags").value.split(",").map(tag => tag.trim()).filter(Boolean),
    description: $("#edit-description").value,
  };
}

function updateVersionWarning() {
  $("#version-warning").hidden = !hasUnversionedChanges(originalGameDetails, readGameDetails());
}

function selectedEditorGame() {
  return state.games.find((game) => gameId(game) === state.editorGameId);
}

function setEditorTab(tabName) {
  $$(".editor-tab").forEach((button) => button.classList.toggle("active", button.dataset.editorTab === tabName));
  $$(".editor-panel").forEach((panel) => panel.classList.toggle("active", panel.dataset.editorPanel === tabName));
  if (tabName === "ranking" && state.rankingLoadedFor !== state.editorGameId) loadEditorRankings();
  if (tabName === "comments") renderGameComments();
}

function openGameManager(game, tabName = "details") {
  state.editorGameId = gameId(game);
  state.rankingLoadedFor = null;
  state.replacementFile = null;
  $("#game-editor-heading").textContent = game.title || state.editorGameId;
  $("#game-editor-id").textContent = state.editorGameId;
  $("#game-comment-search").value = "";
  $("#replacement-upload-input").value = "";
  $("#replacement-file-name").textContent = "ZIPはまだ選択されていません";
  $("#replace-game-upload").disabled = true;
  $("#ranking-fields").innerHTML = '<p class="empty compact">読み込んでいます...</p>';
  fillGameDetails(game);
  renderGameComments();
  $("#game-modal").classList.remove("hidden");
  setEditorTab(tabName);
}

function closeGameManager() {
  $("#game-modal").classList.add("hidden");
  state.editorGameId = null;
  state.rankingLoadedFor = null;
  state.replacementFile = null;
}

function rankingField(index, board = {}) {
  const field = document.createElement("fieldset");
  field.className = "ranking-field";
  field.innerHTML = `
    <legend>ランキング ${index + 1}</legend>
    <label class="ranking-enabled"><input type="checkbox" data-role="enabled"> 使用する</label>
    <div class="form-grid">
      <label class="wide"><span>表示名</span><input data-role="name" maxlength="40" placeholder="ハイスコア"></label>
      <label class="wide"><span>並び順</span><select data-role="order"><option value="high_score">高得点順</option><option value="low_score">低タイム順</option></select></label>
    </div>`;
  const enabled = field.querySelector('[data-role="enabled"]');
  const controls = [...field.querySelectorAll('[data-role="name"], [data-role="order"]')];
  enabled.checked = board.enabled ?? Boolean(board.id);
  field.querySelector('[data-role="name"]').value = board.name || `ランキング${index + 1}`;
  field.querySelector('[data-role="order"]').value = board.order || "high_score";

  const syncEnabledState = () => {
    for (const control of controls) {
      control.disabled = !enabled.checked;
    }
  };
  enabled.addEventListener("change", syncEnabledState);
  syncEnabledState();
  return field;
}

async function loadEditorRankings() {
  const id = state.editorGameId;
  if (!id) return;
  const fields = $("#ranking-fields");
  fields.innerHTML = '<p class="empty compact">読み込んでいます...</p>';
  try {
    const boards = await api(`/api/leaderboards/${encodeURIComponent(id)}`);
    if (state.editorGameId !== id) return;
    $("#ranking-game-id").value = id;
    fields.replaceChildren(rankingField(0, boards[0]), rankingField(1, boards[1]));
    state.rankingLoadedFor = id;
  } catch (error) {
    const message = document.createElement("p");
    message.className = "empty compact error-text";
    message.textContent = error.message;
    fields.replaceChildren(message);
    toast(error.message, true);
  }
}

function readRankingField(field) {
  return {
    name: field.querySelector('[data-role="name"]').value.trim(),
    order: field.querySelector('[data-role="order"]').value,
    enabled: field.querySelector('[data-role="enabled"]').checked,
  };
}

async function saveRankings(event) {
  event.preventDefault();
  const id = $("#ranking-game-id").value;
  const leaderboards = $$(".ranking-field").map(readRankingField);
  const button = $("#save-ranking");
  button.disabled = true;

  try {
    const result = await api(`/api/leaderboards/${encodeURIComponent(id)}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ leaderboards }),
    });
    toast(result.message);
    log(result.message);
  } catch (error) {
    toast(error.message, true);
    log(`ランキング設定失敗: ${error.message}`);
  } finally {
    button.disabled = false;
  }
}

async function saveGame(event) {
  event.preventDefault();
  const id = $("#edit-id").value;
  const button = $("#save-edit");
  const payload = readGameDetails();
  button.disabled = true;
  try {
    const result = await api(`/api/games/${encodeURIComponent(id)}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    });
    toast(result.message); log(result.message); await loadAll();
    const updated = selectedEditorGame();
    if (updated) {
      $("#game-editor-heading").textContent = updated.title || id;
      fillGameDetails(updated);
    }
  } catch (error) { toast(error.message, true); log(`ゲーム更新失敗: ${error.message}`); }
  finally { button.disabled = false; }
}

async function deleteGame(game, button) {
  const id = gameId(game);
  if (!confirm(`「${game.title || id}」を配布一覧から完全に削除しますか？\n関連コメントも削除されます。`)) return;
  button.disabled = true;
  try {
    const result = await api(`/api/games/${encodeURIComponent(id)}`, { method: "DELETE" });
    toast(result.message); log(result.message); await loadAll();
  } catch (error) { toast(error.message, true); log(`ゲーム削除失敗: ${error.message}`); button.disabled = false; }
}

let uploadFiles = [];
let uploadBusy = false;

function selectUploadFiles(files) {
  if (uploadBusy || !files.length) return;
  const invalid = files.filter(file => !file.name.toLowerCase().endsWith(".zip"));
  uploadFiles = files.filter(file => file.name.toLowerCase().endsWith(".zip"));
  if (invalid.length) toast(`ZIP以外の${invalid.length}件は選択されませんでした`, true);
  $("#upload-file-list").replaceChildren(...uploadFiles.map(file => {
    const row = document.createElement("li");
    const name = document.createElement("strong"); name.textContent = file.name;
    const status = document.createElement("small"); status.textContent = `${(file.size / 1024 / 1024).toFixed(1)} MB · 待機中`;
    row.append(name, status);
    return row;
  }));
  $("#upload-summary").textContent = uploadFiles.length ? `${uploadFiles.length}件のZIPを選択しました` : "ZIPファイルを選択してください";
  $("#start-upload").disabled = !uploadFiles.length;
  $("#start-upload").textContent = "アップロードする";
}

async function uploadGames() {
  if (uploadBusy || !uploadFiles.length) return;
  uploadBusy = true;
  const button = $("#start-upload");
  button.disabled = true;
  $("#close-upload").disabled = true;
  $("#game-upload-input").disabled = true;
  $("#upload-dropzone").setAttribute("aria-disabled", "true");
  const files = [...uploadFiles];
  try {
    const results = await uploadFileBatch(files, async file => {
      const form = new FormData(); form.append("game", file, file.name);
      log(`${file.name} のアップロードを開始しました`);
      return api("/api/games/upload", { method: "POST", body: form });
    }, (index, status, message) => {
      const row = $("#upload-file-list").children[index];
      row.dataset.status = status;
      row.querySelector("small").textContent = status === "uploading" ? "アップロード中..." : message;
      button.textContent = `アップロード中 ${index + 1} / ${files.length}`;
      $("#upload-summary").textContent = `${index + 1} / ${files.length}件を処理中`;
      if (status !== "uploading") log(`${files[index].name}: ${status === "error" ? "失敗: " : ""}${message}`);
    });
    const success = results.filter(result => result.ok).length;
    const summary = `完了: ${success}件成功・${results.length - success}件失敗`;
    $("#upload-summary").textContent = summary;
    toast(summary, success !== results.length);
    // 成功したZIPは再送せず、失敗分だけを再試行できるよう残す。
    uploadFiles = files.filter((_, index) => !results[index].ok);
    if (success) {
      try { await loadAll(); }
      catch (error) { toast(`登録済みですが一覧の再読込に失敗しました: ${error.message}`, true); }
    }
  } finally {
    uploadBusy = false;
    button.disabled = !uploadFiles.length;
    button.textContent = uploadFiles.length ? "失敗したZIPを再試行" : "アップロードする";
    $("#close-upload").disabled = false;
    $("#game-upload-input").disabled = false;
    $("#upload-dropzone").removeAttribute("aria-disabled");
  }
}

function closeUpload() {
  if (uploadBusy) return;
  $("#upload-modal").classList.add("hidden");
  $("#upload-game").focus();
}

function selectReplacementFile(file) {
  if (!file) return;
  if (!file.name.toLowerCase().endsWith(".zip")) {
    state.replacementFile = null;
    $("#replacement-file-name").textContent = "ZIPファイルを選択してください";
    $("#replace-game-upload").disabled = true;
    toast("ZIPファイルを選択してください", true);
    return;
  }
  state.replacementFile = file;
  $("#replacement-file-name").textContent = `${file.name} (${(file.size / 1024 / 1024).toFixed(1)} MB)`;
  $("#replace-game-upload").disabled = false;
}

async function replaceGameArchive(button) {
  const file = state.replacementFile;
  const id = state.editorGameId;
  if (!file || !id) return;
  const form = new FormData(); form.append("game", file, file.name);
  button.disabled = true;
  button.textContent = "ZIPを更新中...";
  log(`${id}: ${file.name} の再アップロードを開始しました`);
  try {
    const result = await api(`/api/games/${encodeURIComponent(id)}/upload`, { method: "POST", body: form });
    toast(result.message); log(result.message); await loadAll();
    state.replacementFile = null;
    $("#replacement-upload-input").value = "";
    $("#replacement-file-name").textContent = "更新が完了しました";
  } catch (error) {
    toast(error.message, true); log(`ZIP更新失敗: ${error.message}`);
  } finally {
    button.textContent = "このゲームのZIPを更新";
    button.disabled = !state.replacementFile;
  }
}

async function deleteComment(item, button) {
  if (!confirm(`${item.author || "匿名"} のコメントを削除しますか？`)) return;
  button.disabled = true;
  try {
    const result = await api(`/api/comments/${encodeURIComponent(item.id)}`, { method: "DELETE" });
    state.comments = state.comments.filter((comment) => comment.id !== item.id);
    if (state.editorGameId) renderGameComments();
    $("#comment-count").textContent = state.comments.length; toast(result.message); log(`${item.game_id}: ${result.message}`);
  } catch (error) { toast(error.message, true); button.disabled = false; }
}

async function rescan(button) {
  button.disabled = true; button.textContent = "スキャン中...";
  try { const result = await api("/api/rescan", { method: "POST" }); toast(result.message); log(result.message); await loadAll(); }
  catch (error) { toast(error.message, true); log(`再スキャン失敗: ${error.message}`); }
  finally { button.disabled = false; button.textContent = "ゲームを再スキャン"; }
}

$$('.nav-item').forEach((button) => button.addEventListener("click", () => {
  $$('.nav-item').forEach((item) => item.classList.toggle("active", item === button));
  $$('.view').forEach((view) => view.classList.toggle("active", view.id === `view-${button.dataset.view}`));
  const titles = { dashboard: "ダッシュボード", games: "配布ゲーム", clients: "クライアント" };
  $("#page-title").textContent = titles[button.dataset.view] || button.textContent.trim();
  if (button.dataset.view === "clients") loadClients();
}));
$("#refresh-all").addEventListener("click", loadAll);
$("#rescan").addEventListener("click", (event) => rescan(event.currentTarget));
$("#upload-game").addEventListener("click", () => {
  $("#upload-modal").classList.remove("hidden");
  $("#upload-dropzone").focus();
});
$("#close-upload").addEventListener("click", closeUpload);
$("#upload-modal").addEventListener("click", event => { if (event.target === $("#upload-modal")) closeUpload(); });
$("#upload-modal").addEventListener("keydown", event => { if (event.key === "Escape") closeUpload(); });
$("#start-upload").addEventListener("click", () => {
  // 再試行時にも行とファイルを一対一に揃える。
  selectUploadFiles(uploadFiles);
  void uploadGames();
});
bindFileDropzone($("#upload-dropzone"), $("#game-upload-input"), selectUploadFiles, () => uploadBusy);
$("#game-search").addEventListener("input", renderGames);
$("#edit-form").addEventListener("submit", saveGame);
$("#edit-form").addEventListener("input", updateVersionWarning);
$("#ranking-form").addEventListener("submit", saveRankings);
$("#close-game-editor").addEventListener("click", closeGameManager);
$("#game-modal").addEventListener("click", (event) => { if (event.target === $("#game-modal")) closeGameManager(); });
$$('.editor-tab').forEach((button) => button.addEventListener("click", () => setEditorTab(button.dataset.editorTab)));
$("#game-comment-search").addEventListener("input", renderGameComments);
bindFileDropzone($("#archive-dropzone"), $("#replacement-upload-input"), files => {
  if (files.length > 1) { toast("このゲームの更新用ZIPは1件だけ選択してください", true); return; }
  selectReplacementFile(files[0]);
});
$("#replace-game-upload").addEventListener("click", (event) => replaceGameArchive(event.currentTarget));
$("#login-form").addEventListener("submit", async (event) => {
  event.preventDefault(); state.token = $("#token").value; sessionStorage.setItem("admin-token", state.token);
  $("#login-error").textContent = "";
  await loadAll();
});

loadAll();
setInterval(() => {
  if (state.status) { state.status.uptime_seconds += 30; renderStatus(); }
}, 30000);
setInterval(() => {
  if ($("#view-clients").classList.contains("active")) loadClients();
}, 1000);
