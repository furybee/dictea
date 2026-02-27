import { useState, useEffect, useCallback } from "react";

export type UpdateStatus = "idle" | "available" | "downloading" | "done";

interface UpdateHandle {
  downloadAndInstall: () => Promise<void>;
  version: string;
  available: boolean;
}

export function useUpdater() {
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [version, setVersion] = useState<string>("");
  const [update, setUpdate] = useState<UpdateHandle | null>(null);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const u = await check();
        if (!cancelled && u?.available) {
          setUpdate(u);
          setVersion(u.version);
          setStatus("available");
        }
      } catch {
        // Fail silently — no update check should block the app
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const checkAndDownload = useCallback(async () => {
    if (!update) return;
    try {
      setStatus("downloading");
      await update.downloadAndInstall();
      setStatus("done");
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      setStatus("available");
    }
  }, [update]);

  const dismiss = useCallback(() => {
    setStatus("idle");
  }, []);

  return { status, version, checkAndDownload, dismiss };
}
