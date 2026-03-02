import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function useAudioDevices() {
  const [devices, setDevices] = useState<string[]>([]);
  const [level, setLevel] = useState(0);
  const previewActive = useRef(false);

  const refreshDevices = useCallback(async () => {
    try {
      const list = await invoke<string[]>("list_audio_devices");
      setDevices(list);
    } catch (e) {
      console.error("Failed to list devices:", e);
    }
  }, []);

  const startPreview = useCallback(async (deviceName: string) => {
    try {
      await invoke("start_mic_preview", { deviceName });
      previewActive.current = true;
    } catch (e) {
      console.error("Failed to start mic preview:", e);
    }
  }, []);

  const stopPreview = useCallback(async () => {
    if (!previewActive.current) return;
    try {
      await invoke("stop_mic_preview");
      previewActive.current = false;
      setLevel(0);
    } catch (e) {
      console.error("Failed to stop mic preview:", e);
    }
  }, []);

  useEffect(() => {
    const unlisten = listen<number>("mic_preview_level", (event) => {
      setLevel(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return { devices, level, refreshDevices, startPreview, stopPreview };
}
