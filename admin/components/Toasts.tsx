// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";

type ToastKind = "success" | "error" | "info";

interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

type Push = (kind: ToastKind, message: string) => void;

const ToastContext = createContext<Push>(() => {});

const ALERT_CLASS: Record<ToastKind, string> = {
  success: "alert-success",
  error: "alert-error",
  info: "alert-info",
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);

  const dismiss = useCallback((id: number) => setToasts((list) => list.filter((t) => t.id !== id)), []);

  const push = useCallback<Push>(
    (kind, message) => {
      const id = nextId.current++;
      setToasts((list) => [...list.slice(-4), { id, kind, message }]);
      window.setTimeout(() => dismiss(id), kind === "error" ? 8000 : 4000);
    },
    [dismiss],
  );

  return (
    <ToastContext.Provider value={push}>
      {children}
      <div className="toast toast-end toast-bottom z-50" aria-live="polite">
        {toasts.map((t) => (
          <div
            key={t.id}
            role={t.kind === "error" ? "alert" : "status"}
            className={`alert ${ALERT_CLASS[t.kind]} max-w-sm whitespace-normal shadow-lg`}
          >
            <span className="break-words">{t.message}</span>
            <button
              type="button"
              className="btn btn-ghost btn-xs"
              aria-label="Dismiss notification"
              onClick={() => dismiss(t.id)}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const push = useContext(ToastContext);
  return useMemo(
    () => ({
      success: (message: string) => push("success", message),
      error: (message: string) => push("error", message),
      info: (message: string) => push("info", message),
    }),
    [push],
  );
}
