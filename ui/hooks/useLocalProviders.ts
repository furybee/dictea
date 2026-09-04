import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface ParakeetStatus {
  downloaded: boolean;
  downloading: boolean;
  path: string;
}

export interface AppleStatus {
  availability: string;
  message: string;
}

/// The two providers that need no key still need setting up — one downloads a
/// model, the other depends on the OS. Both pages need to know: Providers to
/// show the setup, Engine to tell a usable option from one that is not.
export function useLocalProviders() {
  const [parakeet, setParakeet] = useState<ParakeetStatus | null>(null);
  const [apple, setApple] = useState<AppleStatus | null>(null);

  const refresh = useCallback(() => {
    invoke<ParakeetStatus>("parakeet_model_status").then(setParakeet).catch(console.error);
    invoke<AppleStatus>("apple_intelligence_status").then(setApple).catch(console.error);
  }, []);

  useEffect(refresh, [refresh]);

  return {
    parakeet,
    apple,
    refresh,
    parakeetReady: parakeet?.downloaded ?? false,
    appleReady: apple?.availability === "available",
    // Off macOS the option can never work, so it is not merely unconfigured
    appleSupported: apple !== null && apple.availability !== "unsupported_os",
  };
}
