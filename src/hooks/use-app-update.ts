import { useState, useCallback, useRef } from "react"
import { track } from "@/lib/analytics"

export type UpdateStatus =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "up-to-date" }
  | { status: "downloading"; progress: number }
  | { status: "installing" }
  | { status: "ready" }
  | { status: "error"; message: string }

interface UseAppUpdateReturn {
  updateStatus: UpdateStatus
  triggerInstall: () => void
  checkForUpdates: () => void
}

/**
 * Auto-updater disabled — tauri-plugin-updater removed for Windows ARM64 compatibility.
 */
export function useAppUpdate(): UseAppUpdateReturn {
  const [updateStatus] = useState<UpdateStatus>({ status: "idle" })

  const checkForUpdates = useCallback(() => {
    // updater plugin removed — no-op
  }, [])

  const triggerInstall = useCallback(() => {
    // updater plugin removed — no-op
  }, [])

  return { updateStatus, triggerInstall, checkForUpdates }
}
