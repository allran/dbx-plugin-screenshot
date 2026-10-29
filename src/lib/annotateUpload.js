const CHUNK_BYTES = 360_000;

function bytesToBase64(bytes) {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/**
 * Upload an annotated image Blob (JPEG or PNG) in chunks and overwrite the item.
 * @param {string} itemId
 * @param {Blob} blob
 */
export async function uploadAnnotatedBlob(itemId, blob) {
  const begin = await window.dbxPlugin.invoke("screenshot/annotate/begin", { id: itemId });
  const sessionId = begin?.sessionId;
  if (!sessionId) {
    throw new Error("Annotate session failed");
  }

  try {
    const buffer = new Uint8Array(await blob.arrayBuffer());
    let index = 0;
    for (let offset = 0; offset < buffer.length; offset += CHUNK_BYTES) {
      const slice = buffer.subarray(offset, Math.min(offset + CHUNK_BYTES, buffer.length));
      await window.dbxPlugin.invoke("screenshot/annotate/chunk", {
        sessionId,
        index,
        dataBase64: bytesToBase64(slice),
      });
      index += 1;
    }
    const committed = await window.dbxPlugin.invoke("screenshot/annotate/commit", { sessionId });
    if (!committed?.item) {
      throw new Error("Annotate commit failed");
    }
    return committed.item;
  } catch (error) {
    try {
      await window.dbxPlugin.invoke("screenshot/annotate/abort", { sessionId });
    } catch {
      // ignore abort errors
    }
    throw error;
  }
}
