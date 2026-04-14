import { useEffect } from "react";
import { useLocation } from "react-router-dom";

import { setDebugEnabled, syncRouteDebugState, toggleDebugEnabled } from "../../app/debug";

export function useDebugBootstrap() {
  const location = useLocation();

  useEffect(() => {
    syncRouteDebugState();
  }, [location.pathname, location.search, location.hash]);

  useEffect(() => {
    const enableFromFlag = window.location.search.includes("debug=1") || window.location.hash.includes("debug=1");
    if (enableFromFlag) {
      setDebugEnabled(true);
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === "d") {
        event.preventDefault();
        toggleDebugEnabled();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
