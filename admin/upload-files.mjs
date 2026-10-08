// ファイル選択とドロップを同じ入口へまとめる。無効中は選択を変更しない。
export function bindFileDropzone(zone, input, onFiles, isDisabled = () => false) {
  const choose = () => { if (!isDisabled()) input.click(); };
  zone.addEventListener("click", choose);
  zone.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") { event.preventDefault(); choose(); }
  });
  input.addEventListener("change", () => {
    if (!isDisabled()) onFiles([...input.files]);
    input.value = "";
  });
  for (const name of ["dragenter", "dragover"]) {
    zone.addEventListener(name, (event) => {
      event.preventDefault();
      if (!isDisabled()) zone.classList.add("dragging");
    });
  }
  zone.addEventListener("dragleave", (event) => {
    if (!zone.contains(event.relatedTarget)) zone.classList.remove("dragging");
  });
  zone.addEventListener("drop", (event) => {
    event.preventDefault();
    zone.classList.remove("dragging");
    if (!isDisabled()) onFiles([...(event.dataTransfer?.files || [])]);
  });
}

// 名前表示と選択解除ボタン。解除はローカルの選択にだけ作用する。
export function createUploadFileRow(document, file, onRemove, isDisabled = () => false) {
  const row = document.createElement("li");
  const heading = document.createElement("div"); heading.className = "upload-file-heading";
  const name = document.createElement("strong"); name.textContent = file.name;
  const remove = document.createElement("button");
  remove.type = "button"; remove.className = "button secondary upload-remove-file";
  remove.textContent = "選択から外す";
  remove.setAttribute("aria-label", `${file.name}を選択から外す`);
  remove.addEventListener("click", () => {
    if (remove.disabled || isDisabled()) return;
    onRemove(file);
  });
  heading.append(name, remove);
  const status = document.createElement("small");
  status.textContent = `${(file.size / 1024 / 1024).toFixed(1)} MB · 待機中`;
  row.append(heading, status);
  return row;
}

// 一つの失敗で残りのZIPを中断せず、各ファイルの結果を返す。
export async function uploadFileBatch(files, upload, onStatus = () => {}) {
  const results = [];
  for (const [index, file] of files.entries()) {
    onStatus(index, "uploading");
    try {
      const result = await upload(file);
      results.push({ ok: true, result });
      onStatus(index, "complete", result.message);
    } catch (error) {
      results.push({ ok: false, error });
      onStatus(index, "error", error.message || String(error));
    }
  }
  return results;
}
