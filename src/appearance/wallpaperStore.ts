/**
 * DEV-0064 §8/§13 · 自定义壁纸存储（IndexedDB）。
 *
 * - DB `higher-appearance` / store `wallpaper` / key `active`。
 * - 保存原始 Blob + MIME + file name + updated_at（§8）。
 * - 禁止 Base64 塞 localStorage（容量小、大图易失败）。
 * - 生命周期：启动 load → createObjectURL → 应用；退出 revoke；损坏 → 忽略回默认不崩溃（§13）。
 */

const DB_NAME = "higher-appearance";
const STORE = "wallpaper";
const KEY = "active";

export interface WallpaperRecord {
  blob: Blob;
  mime: string;
  name: string;
  updated_at: string;
}

/** §7 允许格式与大小上限。 */
export const WALLPAPER_ACCEPT = ["image/png", "image/jpeg", "image/webp"];
export const WALLPAPER_MAX_BYTES = 20 * 1024 * 1024;

export function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains(STORE)) {
        req.result.createObjectStore(STORE);
      }
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error("IndexedDB open failed"));
  });
}

/** 保存（替换语义：成功写入后旧记录自然覆盖 §13）。 */
export async function saveWallpaper(blob: Blob, name: string): Promise<WallpaperRecord> {
  const db = await openDb();
  const rec: WallpaperRecord = {
    blob,
    mime: blob.type,
    name,
    updated_at: new Date().toISOString(),
  };
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).put(rec, KEY);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("IndexedDB write failed"));
    tx.onabort = () => reject(tx.error ?? new Error("IndexedDB write aborted"));
  });
  db.close();
  return rec;
}

/** 读取（无 / 损坏 → null，不抛错 §13）。 */
export async function loadWallpaper(): Promise<WallpaperRecord | null> {
  try {
    const db = await openDb();
    const rec = await new Promise<WallpaperRecord | undefined>((resolve, reject) => {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).get(KEY);
      req.onsuccess = () => resolve(req.result as WallpaperRecord | undefined);
      req.onerror = () => reject(req.error);
    });
    db.close();
    if (!rec || !(rec.blob instanceof Blob) || rec.blob.size <= 0) return null;
    return rec;
  } catch {
    return null;
  }
}

export async function deleteWallpaper(): Promise<void> {
  const db = await openDb();
  await new Promise<void>((resolve, reject) => {
    const tx = db.transaction(STORE, "readwrite");
    tx.objectStore(STORE).delete(KEY);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error("IndexedDB delete failed"));
  });
  db.close();
}

// =============== Object URL 生命周期（模块级单例） ===============

let currentUrl: string | null = null;

/** 应用壁纸到 :root（--h-wallpaper-image；只由 .h-wallpaper-layer 消费）。 */
export function applyWallpaperUrl(url: string | null): void {
  const root = document.documentElement;
  if (url) {
    root.style.setProperty("--h-wallpaper-image", `url("${url}")`);
    root.setAttribute("data-h-wallpaper", "on");
  } else {
    root.style.removeProperty("--h-wallpaper-image");
    root.removeAttribute("data-h-wallpaper");
  }
}

function releaseUrl(): void {
  if (currentUrl) {
    URL.revokeObjectURL(currentUrl);
    currentUrl = null;
  }
}

/** 启动恢复：load → createObjectURL → 应用（§13）。 */
export async function restoreWallpaper(): Promise<WallpaperRecord | null> {
  const rec = await loadWallpaper();
  releaseUrl();
  if (!rec) {
    applyWallpaperUrl(null);
    return null;
  }
  currentUrl = URL.createObjectURL(rec.blob);
  applyWallpaperUrl(currentUrl);
  return rec;
}

/** 导入/替换成功后调用。 */
export async function setWallpaper(blob: Blob, name: string): Promise<WallpaperRecord> {
  const rec = await saveWallpaper(blob, name); // 失败即抛出 → 上层保留原壁纸（§13）
  releaseUrl();
  currentUrl = URL.createObjectURL(rec.blob);
  applyWallpaperUrl(currentUrl);
  return rec;
}

/** 删除：清 DB + revoke + 回颜色氛围背景（§13）。 */
export async function removeWallpaper(): Promise<void> {
  await deleteWallpaper();
  releaseUrl();
  applyWallpaperUrl(null);
}

/**
 * 页面卸载释放（App 挂载时注册 beforeunload）。
 * DEV-0064R.2 §69：返回 cleanup 供 React effect 卸载时 removeEventListener
 * （React.StrictMode 双挂载不再泄漏 listener；cleanup 只移除 listener，不 revoke URL
 * ——真实窗口 unload 才 release）。
 */
export function registerWallpaperUnload(): () => void {
  const handler = () => releaseUrl();
  window.addEventListener("beforeunload", handler);
  return () => {
    window.removeEventListener("beforeunload", handler);
  };
}

/** §7 校验：MIME 白名单 + ≤20MB；返回人话错误（null = 通过）。 */
export function validateWallpaperFile(file: File): string | null {
  if (!WALLPAPER_ACCEPT.includes(file.type)) {
    return "仅支持 PNG / JPG / WEBP 图片。";
  }
  if (file.size > WALLPAPER_MAX_BYTES) {
    return "图片超过 20 MB，请选择更小的图片。";
  }
  return null;
}
