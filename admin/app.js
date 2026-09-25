const state = {
  games: [],
  comments: [],
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

function renderComments() {
  const query = $("#comment-search").value.trim().toLowerCase();
  const comments = state.comments.filter((item) => `${item.game_id} ${item.author} ${item.content}`.toLowerCase().includes(query));
  const list = $("#comments-list");
  list.replaceChildren();
  if (!comments.length) {
    const empty = document.createElement("p"); empty.className = "empty"; empty.textContent = "コメントがありません"; list.append(empty); return;
  }
  for (const item of comments) {
    list.append(createCommentRow(item));
  }
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
    renderStatus(); renderGames(); renderComments();
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
  $("#edit-description").value = game.description || "";
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
      <label><span>ID（半角英数字・_・-）</span><input data-role="id" maxlength="32" placeholder="score"></label>
      <label><span>表示名</span><input data-role="name" maxlength="40" placeholder="ハイスコア"></label>
      <label class="wide"><span>並び順</span><select data-role="order"><option value="high_score">高得点順（大きい方が上）</option><option value="low_score">低タイム順（小さい方が上）</option></select></label>
    </div>`;
  const enabled = field.querySelector('[data-role="enabled"]');
  const controls = [...field.querySelectorAll('[data-role="id"], [data-role="name"], [data-role="order"]')];
  enabled.checked = Boolean(board.id);
  field.querySelector('[data-role="id"]').value = board.id || "";
  field.querySelector('[data-role="name"]').value = board.name || "";
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
    id: field.querySelector('[data-role="id"]').value.trim(),
    name: field.querySelector('[data-role="name"]').value.trim(),
    order: field.querySelector('[data-role="order"]').value,
  };
}

async function saveRankings(event) {
  event.preventDefault();
  const id = $("#ranking-game-id").value;
  const leaderboards = $$(".ranking-field")
    .filter((field) => field.querySelector('[data-role="enabled"]').checked)
    .map(readRankingField);
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
  const payload = {
    title: $("#edit-title").value,
    version: $("#edit-version").value,
    author: $("#edit-author").value,
    latest_update: $("#edit-date").value,
    tags: $("#edit-tags").value.split(",").map((tag) => tag.trim()).filter(Boolean),
    description: $("#edit-description").value,
  };
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

async function uploadGame(file, button) {
  if (!file) return;
  if (!file.name.toLowerCase().endsWith(".zip")) { toast("ZIPファイルを選択してください", true); return; }
  const form = new FormData(); form.append("game", file, file.name);
  button.disabled = true; button.textContent = "アップロード中...";
  log(`${file.name} のアップロードを開始しました`);
  try {
    const result = await api("/api/games/upload", { method: "POST", body: form });
    toast(result.message); log(`${file.name}: ${result.message}`); await loadAll();
  } catch (error) { toast(error.message, true); log(`アップロード失敗: ${error.message}`); }
  finally { button.disabled = false; button.textContent = "ZIPをアップロード"; $("#game-upload-input").value = ""; }
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
    state.comments = state.comments.filter((comment) => comment.id !== item.id); renderComments();
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
  const titles = { dashboard: "ダッシュボード", games: "配布ゲーム", comments: "コメント管理" };
  $("#page-title").textContent = titles[button.dataset.view] || button.textContent.trim();
}));
$("#refresh-all").addEventListener("click", loadAll);
$("#rescan").addEventListener("click", (event) => rescan(event.currentTarget));
$("#upload-game").addEventListener("click", () => $("#game-upload-input").click());
$("#game-upload-input").addEventListener("change", (event) => uploadGame(event.target.files?.[0], $("#upload-game")));
$("#game-search").addEventListener("input", renderGames);
$("#comment-search").addEventListener("input", renderComments);
$("#edit-form").addEventListener("submit", saveGame);
$("#ranking-form").addEventListener("submit", saveRankings);
$("#close-game-editor").addEventListener("click", closeGameManager);
$("#game-modal").addEventListener("click", (event) => { if (event.target === $("#game-modal")) closeGameManager(); });
$$('.editor-tab').forEach((button) => button.addEventListener("click", () => setEditorTab(button.dataset.editorTab)));
$("#game-comment-search").addEventListener("input", renderGameComments);
$("#archive-dropzone").addEventListener("click", () => $("#replacement-upload-input").click());
$("#archive-dropzone").addEventListener("keydown", (event) => {
  if (event.key === "Enter" || event.key === " ") { event.preventDefault(); $("#replacement-upload-input").click(); }
});
$("#replacement-upload-input").addEventListener("change", (event) => selectReplacementFile(event.target.files?.[0]));
for (const eventName of ["dragenter", "dragover"]) {
  $("#archive-dropzone").addEventListener(eventName, (event) => { event.preventDefault(); $("#archive-dropzone").classList.add("dragging"); });
}
for (const eventName of ["dragleave", "drop"]) {
  $("#archive-dropzone").addEventListener(eventName, (event) => { event.preventDefault(); $("#archive-dropzone").classList.remove("dragging"); });
}
$("#archive-dropzone").addEventListener("drop", (event) => selectReplacementFile(event.dataTransfer?.files?.[0]));
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
