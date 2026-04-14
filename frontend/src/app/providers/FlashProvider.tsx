import { createContext, useContext, useMemo, useState, type ReactNode } from "react";

interface FlashContextValue {
  error: string;
  notice: string;
  clear: () => void;
  showError: (message: string) => void;
  showNotice: (message: string) => void;
}

const FlashContext = createContext<FlashContextValue | null>(null);

export function FlashProvider({ children }: { children: ReactNode }) {
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const value = useMemo<FlashContextValue>(
    () => ({
      error,
      notice,
      clear: () => {
        setNotice("");
        setError("");
      },
      showError: (message) => {
        setNotice("");
        setError(message);
      },
      showNotice: (message) => {
        setError("");
        setNotice(message);
      }
    }),
    [error, notice]
  );

  return <FlashContext.Provider value={value}>{children}</FlashContext.Provider>;
}

export function useFlash() {
  const context = useContext(FlashContext);
  if (!context) {
    throw new Error("useFlash must be used inside FlashProvider");
  }
  return context;
}
