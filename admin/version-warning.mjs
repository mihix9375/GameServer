function normalizedVersion(value) {
  return String(value || "").trim().replace(/^v/i, "");
}

function normalizedDetails(value) {
  return JSON.stringify({
    title: value.title.trim(),
    author: value.author.trim(),
    latest_update: value.latest_update.trim(),
    tags: value.tags.map(tag => tag.trim()).filter(Boolean),
    description: value.description,
  });
}

export function hasUnversionedChanges(original, current) {
  return Boolean(original)
    && normalizedVersion(original.version) === normalizedVersion(current.version)
    && normalizedDetails(original) !== normalizedDetails(current);
}
